use super::*;
use varta_common::language::LanguageProfile as P;
fn compile(source: &str, profile: P) -> Result<Vec<u8>, varta_compiler::CompilerError> {
    varta_compiler::compile_typescript_with_profile(source, profile)
}
fn run(source: &str, expected: &[&str]) {
    let bytes = compile(source, P::CheckedF64Utf16V32).unwrap_or_else(|e| panic!("{e}\n{source}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
fn rejected(source: &str, profile: P) -> String {
    compile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}:\n{source}"))
        .to_string()
}

#[test]
fn class_fields_constructors_and_methods_run() {
    run(
        "class Point{x:number;readonly y:number;label='p';constructor(x:number,y:number){this.x=x;this.y=y;}\
         sum():number{return this.x+this.y;}move(dx:number):void{this.x=this.x+dx;}describe():string{return this.label+String(this.sum());}}\
         export function main():void{const p=new Point(1,2);console.log(p.sum());p.move(5);console.log(p.x);console.log(p.describe());\
         p.label='q';console.log(p.describe());console.log(JSON.stringify(p));const same:Point=p;console.log(same===p);console.log(new Point(0,0)===p);}",
        &[
            "3",
            "6",
            "p8",
            "q8",
            "{\"x\":6,\"y\":2,\"label\":\"q\"}",
            "1",
            "0",
        ],
    );
}
#[test]
fn class_inheritance_overrides_dispatch_and_instanceof() {
    run(
        "class Shape{protected readonly name:string;constructor(name:string){this.name=name;}area():number{return 0;}\
         describe():string{return this.name+'='+String(this.area());}kind():string{return 'shape';}}\
         class Rect extends Shape{w:number;h:number;constructor(w:number,h:number){super('rect');this.w=w;this.h=h;}\
         area():number{return this.w*this.h;}}\
         class Square extends Rect{constructor(side:number){super(side,side);}kind():string{return 'square/'+super.kind();}}\
         class Circle extends Shape{r=1;area():number{return 3*this.r*this.r;}}\
         function total(shapes:Shape[]):number{let sum=0;for(const s of shapes){if(s!==undefined){sum=sum+s.area();}}return sum;}\
         export function main():void{const shapes:Shape[]=[new Rect(2,3),new Square(2),new Circle('c'),new Shape('s')];\
         console.log(total(shapes));for(const s of shapes){if(s instanceof Shape){console.log(s.describe()+' '+s.kind());\
         console.log(s instanceof Rect);console.log(s instanceof Square);console.log(s instanceof Shape);}}\
         const sq=new Square(3);const r:Rect=sq;console.log(r.area());console.log(sq instanceof Rect);console.log(r===sq);}",
        &[
            "13",
            "rect=6 shape",
            "1",
            "0",
            "1",
            "rect=4 square/shape",
            "1",
            "1",
            "1",
            "c=3 shape",
            "0",
            "0",
            "1",
            "s=0 shape",
            "0",
            "0",
            "1",
            "9",
            "1",
            "1",
        ],
    );
}
#[test]
fn class_constructors_initializers_access_and_unions() {
    run(
        "class Counter{private count=0;protected step:number;readonly name:string='c';constructor(step:number){this.step=step;}\
         inc():Counter{this.count=this.count+this.step;return this;}value():number{return this.count;}same(o:Counter):boolean{return o.count===this.count;}}\
         class Fast extends Counter{bonus=10;inc():Counter{this.step=this.step+1;return super.inc();}}\
         class Plain extends Fast{}\
         class List{value:number;next:List|null;constructor(value:number,next:List|null){this.value=value;this.next=next;}\
         sum():number{let total=0;let at:List|null=this;while(at!==null){total=total+at.value;at=at.next;}return total;}}\
         function first(c:Counter|undefined):string{if(c instanceof Fast){return 'fast';}if(c===undefined){return 'none';}return 'counter';}\
         export function main():void{const c=new Counter(2);c.inc().inc();console.log(c.value());const f=new Plain(1);f.inc();f.inc();\
         console.log(f.value());console.log(f.bonus);console.log(f.name);console.log(c.same(new Counter(4).inc()));\
         console.log(new List(1,new List(2,new List(3,null))).sum());console.log(first(f));console.log(first(c));console.log(first(undefined));\
         let maybe:Counter|undefined=undefined;if(maybe===undefined){maybe=f;}if(maybe instanceof Counter){console.log(maybe.value());}}",
        &[
            "4", "5", "10", "c", "1", "6", "fast", "counter", "none", "5",
        ],
    );
}
#[test]
fn class_methods_compose_with_closures_exceptions_and_dispatch_chains() {
    run(
        "class Base{tag():string{return 'base';}call(times:number,sep?:string):string{let out='';for(let i=0;i<times;i=i+1){out=out+this.tag()+(sep===undefined?'':sep);}return out;}\
         each(items:number[]):number[]{return items.map(n=>n+this.tag().length);}\
         fail(on:boolean):number{if(on){throw new RangeError('boom '+this.tag());}return 1;}}\
         class Mid extends Base{}\
         class Leaf extends Mid{tag():string{return 'leaf';}}\
         class Guarded{x:number;constructor(x:number){if(x<0){throw new TypeError('negative');}this.x=x;}}\
         export function main():void{const items:Base[]=[new Base(),new Mid(),new Leaf()];\
         for(const b of items){if(b!==undefined){console.log(b.call(2,','));console.log(JSON.stringify(b.each([1,2])));}}\
         const leaf:Mid=new Leaf();try{leaf.fail(true);}catch(e){if(e instanceof RangeError){console.log(e.message);}}finally{console.log('done');}\
         try{new Guarded(-1);console.log('unreachable');}catch(e){if(e instanceof TypeError){console.log(e.message);}}console.log(new Guarded(3).x);}",
        &[
            "base,base,",
            "[5,6]",
            "base,base,",
            "[5,6]",
            "leaf,leaf,",
            "[5,6]",
            "boom leaf",
            "done",
            "negative",
            "3",
        ],
    );
}
#[test]
fn class_unsupported_forms_fail_with_construct_diagnostics() {
    for (source, expected) in [
        ("class A<T>{}", "generic class"),
        ("abstract class A{}", "abstract class"),
        (
            "interface I{}class A implements I{}",
            "class implements clause",
        ),
        ("class A{[k:string]:number}", "class index signature"),
        ("class A{static x=1}", "static class field"),
        (
            "class A{static get f():number{return 1;}}",
            "static class accessor",
        ),
        ("class A{set x(v:number){}}", "class setter"),
        ("class A{#x=1}", "#private class field"),
        ("class A{#f():void{}}", "#private class method"),
        (
            "class A{constructor(private x:number){}}",
            "constructor parameter property",
        ),
        (
            "class A extends B{}",
            "class extends B, which is not a declared class",
        ),
        ("class A{x?:number}", "optional class field"),
        ("class A{x!:number}", "definite assignment assertion"),
        ("class A{f:()=>void=()=>{}}", "function-valued class field"),
        (
            "class A{y='a'+'b'}",
            "requires a type annotation or a literal initializer",
        ),
        (
            "class A{toJSON():string{return '';}}",
            "class toJSON method",
        ),
        (
            "function f():void{class L{}}",
            "class declaration outside module top level",
        ),
        ("function f():void{const C=class{};}", "class expression"),
        ("class Error{}", "class named like the built-in Error"),
        ("class A extends A{}", "recursive class inheritance"),
        (
            "class A extends B{}class B extends A{}",
            "recursive class inheritance",
        ),
        ("class A{}class A{}", "duplicate type declaration: A"),
        (
            "class A{}function A():void{}",
            "duplicate class and function name: A",
        ),
        ("class A{x=1;x=2}", "duplicate class member: x"),
        ("class A{x=1;x():void{}}", "duplicate class member: x"),
        (
            "class A{x=1}class B{y=1}function f(v:A|B):void{}",
            "multiple record shapes",
        ),
        (
            "class A{x=1}function f():void{const a=varta.json.parse<A>('{}');}",
            "JSON parsing target type",
        ),
        (
            "class A{x=1}function f(a:A):void{console.log(a);}",
            "cannot log class instance",
        ),
        (
            "class A{x=1}function f(a:A):string[]{return Object.keys(a);}",
            "Object.keys requires a closed record",
        ),
        (
            "class A{x=1}function f(a:A):void{const {x}=a;}",
            "object binding pattern requires a closed record",
        ),
        (
            "class A{x=1}function f():void{const a:A={x:1};}",
            "expected class instance, found object",
        ),
        (
            "class A{x=1}function f(a:A):void{a.y=1;}",
            "unknown class member: y",
        ),
        (
            "class A{f():void{}}function g(a:A):void{a.g();}",
            "unknown class method: g",
        ),
        (
            "class A{x=1}function g(a:A):void{a.x();}",
            "class field x is not callable",
        ),
        (
            "class A{x=1}function f():void{new A(1);}",
            "class A constructor expects 0 arguments",
        ),
        (
            "class A{f():void{super.f();}}",
            "super property outside a derived class",
        ),
        (
            "class A{f():void{}}function g(a:A):void{const m=a.f;}",
            "class method value without a call",
        ),
    ] {
        let source = format!("{source}export function main():void{{}}");
        let error = rejected(&source, P::CheckedF64Utf16V32);
        assert!(error.contains(expected), "{source}\n{error}");
    }
}
#[test]
fn class_dispatch_forwards_extended_parameters_and_implicit_constructors() {
    run(
        "class B{a:number;b:string;constructor(a:number,b?:string){this.a=a;this.b=b===undefined?'-':b;}\
         fmt(prefix:string,suffix?:string,...rest:number[]):string{return prefix+String(this.a)+this.b+(suffix===undefined?'':suffix)+String(rest.length);}}\
         class D extends B{fmt(prefix:string,suffix?:string,...rest:number[]):string{let total=0;for(const n of rest){if(n!==undefined){total=total+n;}}\
         return 'D'+super.fmt(prefix,suffix)+String(total);}}\
         class E extends D{}\
         export function main():void{const items:B[]=[new B(1),new D(2,'x'),new E(3)];\
         for(const item of items){if(item!==undefined){console.log(item.fmt('<'));console.log(item.fmt('<','>',4,5));}}}",
        &["<1-0", "<1->2", "D<2x00", "D<2x>09", "D<3-00", "D<3->09"],
    );
    let error = rejected(
        "class B{constructor(...xs:number[]){}}class D extends B{}export function main():void{}",
        P::CheckedF64Utf16V32,
    );
    assert!(
        error.contains("implicit derived constructor of a rest-parameter base"),
        "{error}"
    );
}
#[test]
fn class_static_methods_are_direct_receiver_free_calls() {
    run(
        "class Point{x:number;y:number;constructor(x:number,y:number){this.x=x;this.y=y;}\
         static origin():Point{return new Point(0,0);}private static twice(n:number):number{return n*2;}\
         static scaled(n:number):Point{return new Point(Point.twice(n),n);}sum():number{return this.x+this.y;}}\
         class Labeled extends Point{static origin():Point{return new Labeled(7,7);}}\
         export function main():void{console.log(Point.origin().sum());console.log(Point.scaled(3).sum());\
         console.log(Labeled.scaled(1).sum());console.log(Labeled.origin().sum());console.log(Labeled.origin() instanceof Labeled);}",
        &["0", "9", "3", "14", "1"],
    );
    for (source, expected) in [
        (
            "class A{static f():number{return this.g();}g():number{return 1;}}",
            "this requires an explicit typed function receiver",
        ),
        (
            "class A{private static f():void{}}function g():void{A.f();}",
            "class member f is private to A",
        ),
        (
            "class A{static f():void{}}function g():void{A.h();}",
            "unknown static class method: h",
        ),
        (
            "class A{static f():void{}static f():void{}}",
            "duplicate static class member: f",
        ),
        (
            "class A{f():void{}}function g():void{A.f();}",
            "unknown static class method: f",
        ),
    ] {
        let source = format!("{source}export function main():void{{}}");
        let error = rejected(&source, P::CheckedF64Utf16V32);
        assert!(error.contains(expected), "{source}\n{error}");
    }
}
#[test]
fn class_getters_dispatch_and_stay_out_of_json() {
    run(
        "class Box{protected w:number;constructor(w:number){this.w=w;}get area():number{return this.w*this.w;}get self():Box{return this;}\
         get label():string{return 'box'+String(this.area);}}\
         class Wide extends Box{get area():number{return this.w*10;}}\
         export function main():void{const b:Box=new Wide(2);console.log(b.area);console.log(b.label);console.log(b.self.self.area);\
         console.log(new Box(3).label);console.log(JSON.stringify(b));}",
        &["20", "box20", "20", "box9", "{\"w\":2}"],
    );
    for (source, expected) in [
        (
            "class A{get x():number{return 1;}}function f(a:A):void{a.x=2;}",
            "class getter x has no setter",
        ),
        (
            "class A{get x():number{return 1;}}function f(a:A):void{a.x();}",
            "class getter x is not callable",
        ),
        (
            "class A{get x():number{return 1;}}class B extends A{x():number{return 2;}}",
            "must keep the inherited member kind",
        ),
        (
            "class A{get x():void{}}",
            "class getter x must return a value",
        ),
        (
            "class A{private get x():number{return 1;}}function f(a:A):number{return a.x;}",
            "class member x is private to A",
        ),
        (
            "class A{y:number;get x():number{return 1;}constructor(){this.y=this.x;}}",
            "constructor this may only access declared fields",
        ),
        (
            "class A{get x():number{return 1;}}class B extends A{f():number{return super.x;}}",
            "super property read",
        ),
    ] {
        let source = format!("{source}export function main():void{{}}");
        let error = rejected(&source, P::CheckedF64Utf16V32);
        assert!(error.contains(expected), "{source}\n{error}");
    }
}
#[test]
fn class_member_checks_enforce_access_readonly_and_overrides() {
    for (source, expected) in [
        (
            "class A{readonly x:number=1;f():void{this.x=2;}}",
            "cannot assign to readonly class field x",
        ),
        (
            "class A{readonly x:number=1;}class B extends A{constructor(){super();this.x=2;}}",
            "cannot assign to readonly class field x",
        ),
        (
            "class A{readonly x=1}function f(a:A):void{a.x=3;}",
            "cannot assign to readonly class field x",
        ),
        (
            "class A{readonly x=1}function f(a:A):void{a.x+=3;}",
            "cannot assign to readonly class field x",
        ),
        (
            "class A{private x=1;}function g(a:A):number{return a.x;}",
            "class member x is private to A",
        ),
        (
            "class A{private x=1;}class B extends A{f():number{return this.x;}}",
            "class member x is private to A",
        ),
        (
            "class A{private f():void{}}function g(a:A):void{a.f();}",
            "class member f is private to A",
        ),
        (
            "class A{protected x=1;}function g(a:A):number{return a.x;}",
            "class member x is protected to A",
        ),
        (
            "class A{protected x=1;}class B extends A{}class C extends A{f(b:B):number{return b.x;}}",
            "class member x is protected to A",
        ),
        (
            "class B{f():number{return 1;}}class A extends B{f():string{return 'a';}}",
            "must keep the inherited method signature",
        ),
        (
            "class B{f(x:number):void{}}class A extends B{f():void{}}",
            "must keep the inherited method signature",
        ),
        (
            "class B{private f():void{}}class A extends B{f():void{}}",
            "inherited non-private accessibility",
        ),
        (
            "class B{protected f():void{}}class A extends B{f():void{}}",
            "inherited non-private accessibility",
        ),
    ] {
        let source = format!("{source}export function main():void{{}}");
        let error = rejected(&source, P::CheckedF64Utf16V32);
        assert!(error.contains(expected), "{source}\n{error}");
    }
    // Positive access: private inside the class (also through other instances
    // and closures), protected in subclasses through their own instances.
    run(
        "class A{private secret=1;protected shared=2;peek(o:A):number{const f=(n:number)=>this.secret+n;return f(o.secret);}}\
         class B extends A{read(o:B):number{return o.shared+this.shared;}}\
         export function main():void{const b=new B();console.log(b.peek(new A()));console.log(b.read(new B()));}",
        &["2", "4"],
    );
}
#[test]
fn class_constructor_discipline_guarantees_definite_initialization() {
    for (source, expected) in [
        (
            "class A{x:number;}",
            "class field x is not definitely assigned",
        ),
        (
            "class A{x:number;constructor(c:boolean){if(c){this.x=1;}else{this.x=2;}}}",
            "class field x is not definitely assigned",
        ),
        (
            "class A{x:number;constructor(){this.f();this.x=1;}f():void{}}",
            "constructor this may only access declared fields",
        ),
        (
            "class A{x:number;constructor(){this.x=1;this.f();}f():void{}}",
            "constructor this may only access declared fields",
        ),
        (
            "class A{x:number;constructor(){const s=this;this.x=1;}}",
            "constructor this may only access declared fields",
        ),
        (
            "class A{x:number;constructor(){this.x=1;g(this);}}function g(a:A):void{}",
            "constructor this may only access declared fields",
        ),
        (
            "class A{x:number;y:number;constructor(){this.x=this.y;this.y=1;}}",
            "class field y is read before it is definitely assigned",
        ),
        (
            "class A{x:number;constructor(){this.x=this.x+1;}}",
            "class field x is read before it is definitely assigned",
        ),
        (
            "class A{x:number;constructor(){this.x=1;const f=()=>this.x;}}",
            "constructor closures cannot capture this",
        ),
        (
            "class A{x:number;constructor(){this.x=1;return;}}",
            "return in a class constructor",
        ),
        (
            "class A{x=1;y:number=this.x;}",
            "this is unavailable before super(), in field initializers",
        ),
        (
            "class A{x:number=p;constructor(p:number){}}",
            "field initializer referencing constructor binding p",
        ),
        (
            "class A{x:number;constructor(v:number=this.x){this.x=v;}}",
            "this is unavailable before super()",
        ),
        (
            "class A{x:number=v;constructor(){const v=1;}}",
            "field initializer referencing constructor binding v",
        ),
        (
            "class B{}class A extends B{x:number;constructor(){this.x=1;super();}}",
            "this is unavailable before super()",
        ),
        (
            "class B{}class A extends B{constructor(){}}",
            "derived class constructor without a top-level super(...) statement",
        ),
        (
            "class B{}class A extends B{constructor(c:boolean){if(c){super();}}}",
            "super(...) must be a top-level statement",
        ),
        (
            "class B{}class A extends B{constructor(){super();super();}}",
            "super(...) must be a top-level statement",
        ),
        (
            "class B{f():void{}}class A extends B{constructor(){super();super.f();}}",
            "super property access in a constructor",
        ),
        (
            "class A{constructor(){super();}}",
            "super(...) must be a top-level statement",
        ),
        (
            "class B{}class A extends B{f():void{super();}}",
            "super(...) outside a derived class constructor",
        ),
        (
            "class A{x:number;constructor(){this.x=new.target===undefined?1:2;}}",
            "new.target",
        ),
    ] {
        let source = format!("{source}export function main():void{{}}");
        let error = rejected(&source, P::CheckedF64Utf16V32);
        assert!(error.contains(expected), "{source}\n{error}");
    }
    // Writes may repeat or nest once assigned; reads see assigned and inherited
    // fields; initializers run in order (base before derived, before the body).
    run(
        "class B{a='a';b:string;constructor(b:string){this.b=b+this.a;}}\
         class D extends B{c:string='c';d:string;constructor(){const pre='p';super(pre);this.d=this.a+this.b+this.c;if(this.d.length>3){this.d=this.d+'!';}}}\
         class E extends D{}\
         export function main():void{const e=new E();console.log(e.d);console.log(JSON.stringify(e));}",
        &[
            "apac!",
            "{\"a\":\"a\",\"b\":\"pa\",\"c\":\"c\",\"d\":\"apac!\"}",
        ],
    );
}
#[test]
fn class_declarations_stay_rejected_before_v32() {
    for source in [
        "class A{}export function main():void{}",
        "export class A{x=1}export function main():void{}",
    ] {
        let error = rejected(source, P::CheckedF64Utf16V31);
        assert!(
            error.contains("unsupported top-level declaration"),
            "{error}"
        );
        compile(source, P::CheckedF64Utf16V32).unwrap();
    }
    for source in [
        "export function main():void{class L{}}",
        "export function main():void{const C=class{};}",
    ] {
        let error = rejected(source, P::CheckedF64Utf16V31);
        assert!(
            error.contains("unsupported statement") || error.contains("unsupported expression"),
            "{error}"
        );
    }
}
/// Run with an explicit test-only fuel budget so the churn forces collections.
fn run_collected(source: &str) -> Vec<String> {
    let bytes = compile(source, P::CheckedF64Utf16V32).unwrap_or_else(|e| panic!("{e}\n{source}"));
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(400 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    execution.store.data().output.lines.clone()
}
#[test]
fn class_instances_survive_collection_with_exact_subclass_tracing() {
    // Live instances are held only through base-typed slots, fields, arrays and
    // captured `this`; subclass-only reference fields must still be traced.
    let lines = run_collected(
        r#"
function churn():void{let payload="x";for(let i=0;i<14;i=i+1){payload=payload+payload;}for(let i=0;i<350;i=i+1){const values=[payload+"!",payload+"?"];values.slice();}}
class Node{label:string;constructor(label:string){this.label=label;}text():string{return this.label;}}
class Pair extends Node{right:Node;extra:string;constructor(label:string,right:Node){super(label);this.right=right;this.extra=label+"😀";}
 text():string{return this.label+"/"+this.right.text()+"/"+this.extra;}}
class Holder{items:Node[]=[];first:Node;constructor(first:Node){this.first=first;}
 reader():()=>string{return ()=>this.first.text();}}
function garbage(i:number):void{const p=new Pair(i>0?"g"+"h":"k",new Node("n"));p.text();}
export function main():void{
 const held:Node=new Pair("a"+"b",new Pair("c"+"d",new Node("e"+"f")));
 const holder=new Holder(held);holder.items.push(new Pair("p"+"q",new Node("r"+"s")));const read=holder.reader();
 churn();for(let i=0;i<6000;i=i+1){garbage(i);}churn();
 console.log(held.text());console.log(read());const item=holder.items[0];if(item!==undefined){console.log(item.text());}
 console.log(JSON.stringify(holder));}
"#,
    );
    assert_eq!(
        lines,
        [
            "ab/cd/ef/cd😀/ab😀",
            "ab/cd/ef/cd😀/ab😀",
            "pq/rs/pq😀",
            "{\"items\":[{\"label\":\"pq\",\"right\":{\"label\":\"rs\"},\"extra\":\"pq😀\"}],\"first\":{\"label\":\"ab\",\"right\":{\"label\":\"cd\",\"right\":{\"label\":\"ef\"},\"extra\":\"cd😀\"},\"extra\":\"ab😀\"}}",
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
