//! V38 module evaluation order and top-level `await` (`expression_forms`
//! over `module_evaluation`).
//!
//! Modules are evaluated as ECMAScript's Evaluate() does (§16.2.1.5.3): a
//! depth-first walk from the entry over each module's requested modules in
//! source order (imports and re-exports; `import type` and `export type`
//! declarations request nothing, as TypeScript's `verbatimModuleSyntax` and
//! Node's type stripping erase them), each module after its dependencies.
//! The graph is acyclic (cycles are refused while it loads), so the walk and
//! every quantity it computes are static.
//!
//! A module whose initializers `await` is evaluated as one async function.
//! Without one, the initializers of every module run in that order before
//! the entry body, as before. With one, the program's entry is a synthesized
//! evaluator: it runs the static plan of InnerModuleEvaluation (a module with
//! neither `await` nor a dependency still evaluating asynchronously runs at
//! once; an async one starts), and the completion of an async module runs
//! AsyncModuleExecutionFulfilled at run time: its waiting ancestors whose
//! last pending dependency it was are gathered (through synchronous ones,
//! GatherAvailableAncestors) and run or started in the order they became
//! asynchronous. A failure (AsyncModuleExecutionRejected) marks the module
//! and every waiting ancestor failed; the entry's failure is an unhandled
//! rejection. The entry module calls `main` at the end of its evaluation,
//! as `await main();` appended to it would.
use super::*;
use crate::modules::Graph;
use std::collections::BTreeSet;
use std::path::PathBuf;
use swc_ecma_visit::{Visit, VisitWith};

/// The evaluation of one module: its initializer functions in order and
/// whether its single initializer is async (it `await`s).
#[derive(Default, Clone)]
pub(super) struct ModuleEvaluation {
    pub initializers: Vec<usize>,
    pub asynchronous: bool,
}

/// Whether a module's initializers `await` outside every function value.
pub(super) fn awaits(module: &ast::Module) -> bool {
    struct Awaits(bool);
    impl Visit for Awaits {
        fn visit_await_expr(&mut self, _: &ast::AwaitExpr) {
            self.0 = true;
        }
        fn visit_for_of_stmt(&mut self, node: &ast::ForOfStmt) {
            self.0 |= node.is_await;
            node.visit_children_with(self);
        }
        fn visit_arrow_expr(&mut self, _: &ast::ArrowExpr) {}
        fn visit_function(&mut self, _: &ast::Function) {}
        fn visit_class(&mut self, _: &ast::Class) {}
    }
    let mut found = Awaits(false);
    for item in &module.body {
        let var = match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Var(var))) => var,
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => {
                match &export.decl {
                    ast::Decl::Var(var) => var,
                    _ => continue,
                }
            }
            _ => continue,
        };
        var.visit_with(&mut found);
    }
    found.0
}

/// Whether any module of the program awaits at top level.
pub(super) fn program_awaits(
    graph: &Graph,
    profile: varta_common::language::LanguageProfile,
) -> bool {
    profile.expression_forms()
        && profile.module_evaluation()
        && graph
            .units
            .values()
            .any(|unit| !unit.text && awaits(&unit.ast))
}

/// The modules a module requests, in source order: implicit engine preludes
/// first, then every import and re-export that is not type-only.
fn requests(graph: &Graph, path: &PathBuf) -> Vec<PathBuf> {
    let unit = &graph.units[path];
    // The specifier spans of type-only declarations.
    let mut erased = BTreeSet::new();
    for item in &unit.ast.body {
        match item {
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::Import(import)) if import.type_only => {
                erased.insert(import.src.span.lo.0);
            }
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportNamed(export))
                if export.type_only =>
            {
                if let Some(src) = &export.src {
                    erased.insert(src.span.lo.0);
                }
            }
            _ => {}
        }
    }
    let mut imports: Vec<&crate::modules::Dependency> = unit
        .imports
        .iter()
        .filter(|import| !erased.contains(&import.span.lo.0))
        .collect();
    imports.sort_by_key(|import| import.span.lo.0);
    let mut seen = BTreeSet::new();
    imports
        .into_iter()
        .filter(|import| seen.insert(import.target.clone()))
        .map(|import| import.target.clone())
        .collect()
}

/// Every module in evaluation order (post-order of the source-order
/// depth-first walk from the entry); a text resource is a module that
/// only binds its text.
pub(super) fn order(graph: &Graph) -> Vec<PathBuf> {
    fn visit(graph: &Graph, path: &PathBuf, done: &mut BTreeSet<PathBuf>, out: &mut Vec<PathBuf>) {
        if !done.insert(path.clone()) {
            return;
        }
        for target in requests(graph, path) {
            if graph.units.contains_key(&target) {
                visit(graph, &target, done, out);
            }
        }
        out.push(path.clone());
    }
    let mut done = BTreeSet::new();
    let mut out = Vec::new();
    // Engine preludes are compiler-owned and evaluated before the program.
    for path in &graph.order {
        if crate::modules::engine::is_prelude(path) {
            visit(graph, path, &mut done, &mut out);
        }
    }
    visit(graph, &graph.entry, &mut done, &mut out);
    out
}

/// The program's initializers in evaluation order: those of no module (the
/// template objects) first, then each module's in `order`.
pub(super) fn ordered(graph: &Graph, linked: &module_constants::Linked) -> Vec<usize> {
    let owned: BTreeSet<usize> = linked
        .modules
        .values()
        .flat_map(|module| module.initializers.iter().copied())
        .collect();
    let mut initializers: Vec<usize> = linked
        .initializers
        .iter()
        .copied()
        .filter(|index| !owned.contains(index))
        .collect();
    let order = order(graph);
    // Every module is reached from the entry or is a prelude; the graph's
    // own order keeps any other one rather than dropping it.
    let rest = graph.order.iter().filter(|path| !order.contains(path));
    for path in order.iter().chain(rest) {
        if let Some(module) = linked.modules.get(path) {
            initializers.extend(module.initializers.iter().copied());
        }
    }
    initializers
}

/// The static plan of an evaluation with top-level `await`, by module
/// number (the position in `order`).
struct Plan {
    /// Whether the module is evaluated asynchronously ([[AsyncEvaluation]]).
    evaluating: Vec<bool>,
    /// The module awaits itself ([[HasTLA]]).
    awaits: Vec<bool>,
    /// [[PendingAsyncDependencies]] after the walk.
    pending: Vec<u32>,
    /// [[AsyncParentModules]].
    parents: Vec<Vec<usize>>,
    /// Modules in the order they became asynchronous.
    asynchronous: Vec<usize>,
    /// The walk's actions in order: run a module now or start it.
    actions: Vec<(usize, bool)>,
}

fn plan(graph: &Graph, order: &[PathBuf], modules: &BTreeMap<PathBuf, ModuleEvaluation>) -> Plan {
    let number: BTreeMap<&PathBuf, usize> = order
        .iter()
        .enumerate()
        .map(|(index, path)| (path, index))
        .collect();
    let count = order.len();
    let mut plan = Plan {
        evaluating: vec![false; count],
        awaits: vec![false; count],
        pending: vec![0; count],
        parents: vec![Vec::new(); count],
        asynchronous: Vec::new(),
        actions: Vec::new(),
    };
    // Post-order: every requested module was visited before its importer.
    for (module, path) in order.iter().enumerate() {
        plan.awaits[module] = modules.get(path).is_some_and(|m| m.asynchronous);
        for target in requests(graph, path) {
            let Some(&required) = number.get(&target) else {
                continue;
            };
            if plan.evaluating[required] {
                plan.pending[module] += 1;
                plan.parents[required].push(module);
            }
        }
        if plan.pending[module] > 0 || plan.awaits[module] {
            plan.evaluating[module] = true;
            plan.asynchronous.push(module);
            if plan.pending[module] == 0 {
                plan.actions.push((module, true));
            }
        } else {
            plan.actions.push((module, false));
        }
    }
    plan
}

/// The evaluator's text: `__varta_evaluate()` and its helpers, over the
/// module bodies `__varta_module_<n>` (sync `() => void` or async
/// `() => Promise<void>`).
fn evaluator_text(plan: &Plan, root: usize) -> String {
    let count = plan.evaluating.len();
    let list = |items: &mut dyn Iterator<Item = String>| items.collect::<Vec<_>>().join(", ");
    let state = "s: { pending: number[]; status: number[] }";
    let mut run = String::new();
    let mut start = String::new();
    let mut parents = String::new();
    for module in 0..count {
        if plan.awaits[module] {
            start.push_str(&format!(
                "if (m === {module}) {{ return __varta_module_{module}(); }}\n"
            ));
        } else {
            run.push_str(&format!(
                "if (m === {module}) {{ __varta_module_{module}(); }}\n"
            ));
        }
        if !plan.parents[module].is_empty() {
            parents.push_str(&format!(
                "if (m === {module}) {{ return [{}]; }}\n",
                list(&mut plan.parents[module].iter().map(usize::to_string))
            ));
        }
    }
    let actions: String = plan
        .actions
        .iter()
        .map(|(module, start)| {
            if *start {
                format!("__varta_execute({module}, s);\n")
            } else {
                format!("__varta_run({module});\n")
            }
        })
        .collect();
    let awaiting = list(&mut plan.awaits.iter().map(bool::to_string));
    let asynchronous = list(&mut plan.asynchronous.iter().map(usize::to_string));
    let pending = list(&mut plan.pending.iter().map(u32::to_string));
    let status = list(&mut (0..count).map(|_| "0".to_owned()));
    let unready = list(&mut (0..count).map(|_| "false".to_owned()));
    format!(
        "function __varta_evaluate(): void {{
const s: {{ pending: number[]; status: number[] }} = {{ pending: [{pending}], status: [{status}] }};
{actions}}}
function __varta_run(m: number): void {{
{run}}}
function __varta_start(m: number): Promise<void> {{
{start}return Promise.resolve();
}}
function __varta_parents(m: number): number[] {{
{parents}return [];
}}
function __varta_awaits(m: number): boolean {{
const awaiting: boolean[] = [{awaiting}];
return awaiting[m];
}}
function __varta_execute(m: number, {state}): void {{
__varta_start(m).then(() => {{ __varta_done(m, s); }}, (e) => {{ __varta_fail(m, e, s); }});
}}
function __varta_fail(m: number, e: unknown, {state}): void {{
if (s.status[m] === 2) {{ return; }}
s.status[m] = 2;
for (const p of __varta_parents(m)) {{ __varta_fail(p, e, s); }}
if (m === {root}) {{ __varta_reject(e); }}
}}
function __varta_reject(e: unknown): void {{
Promise.resolve().then(() => {{ throw e; }});
}}
function __varta_gather(m: number, ready: boolean[], {state}): void {{
for (const p of __varta_parents(m)) {{
if (!ready[p] && s.status[p] !== 2) {{
s.pending[p] = s.pending[p] - 1;
if (s.pending[p] === 0) {{
ready[p] = true;
if (!__varta_awaits(p)) {{ __varta_gather(p, ready, s); }}
}}
}}
}}
}}
function __varta_done(m: number, {state}): void {{
if (s.status[m] === 2) {{ return; }}
s.status[m] = 1;
const ready: boolean[] = [{unready}];
__varta_gather(m, ready, s);
const order: number[] = [{asynchronous}];
for (const x of order) {{
if (ready[x] && s.status[x] !== 2) {{
if (__varta_awaits(x)) {{ __varta_execute(x, s); }} else {{
try {{ __varta_run(x); s.status[x] = 1; }} catch (e) {{ __varta_fail(x, e, s); }}
}}
}}
}}
}}
"
    )
}

/// The entry of a program with top-level `await`: compile the evaluator
/// over one body function per module and return its index.
pub(super) fn evaluator(
    graph: &Graph,
    modules: &BTreeMap<PathBuf, ModuleEvaluation>,
    functions: &mut Vec<ir::Function>,
    types: &mut ir::ValueTypes,
    entry: usize,
) -> Result<usize> {
    let order = order(graph);
    let plan = plan(graph, &order, modules);
    let root = order.len() - 1;
    let span = swc_common::DUMMY_SP;
    let source = Source::new(std::path::Path::new("__varta_module_evaluation.ts"), "");
    let promise = promises::intern(Type::Void, types)?;
    // One body per module: its initializers in order (the async one alone).
    let mut symbols = BTreeMap::new();
    for (module, path) in order.iter().enumerate() {
        let evaluation = modules.get(path).cloned().unwrap_or_default();
        let (index, result) = match evaluation.initializers.as_slice() {
            [single] if evaluation.asynchronous => (*single, promise),
            _ => {
                let index = functions.len();
                if index >= types.function_limit() {
                    return Err(invalid("too many functions including module evaluation"));
                }
                functions.push(ir::Function {
                    shape: ir::ParameterShape::fixed(0),
                    self_slot: None,
                    receiver: None,
                    cells: Default::default(),
                    captures: vec![],
                    environment: false,
                    generator: None,
                    source: source.clone(),
                    span,
                    params: vec![],
                    result: Type::Void,
                    locals: vec![],
                    // The entry module calls `main` when it is evaluated.
                    body: evaluation
                        .initializers
                        .iter()
                        .copied()
                        .chain((module == root).then_some(entry))
                        .map(|index| {
                            S::Expr(Expr {
                                span,
                                ty: Type::Void,
                                kind: E::Call(index, vec![]),
                            })
                        })
                        .collect(),
                });
                (index, Type::Void)
            }
        };
        symbols.insert(
            format!("__varta_module_{module}"),
            Signature {
                shape: ir::ParameterShape::fixed(0),
                receiver: None,
                index,
                names: vec![],
                params: vec![],
                result,
            },
        );
    }
    let text = evaluator_text(&plan, root);
    let source = Source::new(std::path::Path::new("__varta_module_evaluation.ts"), &text);
    let module = crate::parse::parse_module(&source, false)?;
    let declarations = declarations::collect(&module, &source, types.profile)?;
    let base = functions.len();
    let mut signatures = Vec::with_capacity(declarations.len());
    for (offset, declaration) in declarations.iter().enumerate() {
        let signature = signature::check(declaration, base + offset, &source, types)?;
        symbols.insert(declaration.ident.sym.to_string(), signature.clone());
        signatures.push(signature);
    }
    let mut generated = BTreeMap::new();
    let mut next = base + declarations.len();
    let mut impacts = BTreeMap::new();
    let mut bodies = Vec::with_capacity(declarations.len());
    for (declaration, signature) in declarations.iter().zip(&signatures) {
        bodies.push(link::body(
            declaration,
            (signature, &Default::default()),
            &symbols,
            &source,
            entry,
            types,
            (&mut generated, &mut next, &mut impacts),
        )?);
    }
    functions.extend(bodies);
    functions.extend(generated.into_values());
    if functions.len() > types.function_limit() {
        return Err(invalid("too many functions including module evaluation"));
    }
    Ok(base)
}
