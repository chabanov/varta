//! Snapshot-aware property copy inference.
use super::*;

impl Lower<'_> {
    pub(in crate::lower) fn snapshot_spread_literal(
        &mut self,
        object: &ast::ObjectLit,
        expected: Option<Type>,
        create_data: bool,
    ) -> Result<Option<Expr>> {
        if !self.types.profile.checked_snapshots() {
            return Ok(None);
        }
        let snapshot_source = object.props.iter().any(|part| {
            let ast::PropOrSpread::Spread(spread) = part else {
                return false;
            };
            self.peek_type(&spread.expr, 0).is_some_and(|ty| {
                unions::members(ty, self.types).into_iter().any(|ty| {
                    matches!(ty, Type::Record(shape) if self.types.subtyping.view_sources(shape)
                        .iter().any(|source| physical(*source, self.types).is_some()))
                })
            })
        });
        if !snapshot_source {
            return Ok(None);
        }
        let mut fields = BTreeMap::new();
        for part in &object.props {
            match part {
                ast::PropOrSpread::Spread(spread) => {
                    let ty = self.peek_type(&spread.expr, 0).ok_or_else(|| {
                        unsupported("snapshot spread requires a known object type")
                    })?;
                    for ty in unions::members(ty, self.types) {
                        let source = match ty {
                            Type::Record(shape) => {
                                self.types.records[shape as usize].fields.clone()
                            }
                            Type::Dictionary(_) => self
                                .types
                                .declared(ty)
                                .map(|d| d.fields.clone())
                                .unwrap_or_default(),
                            Type::Null | Type::Undefined => Vec::new(),
                            _ => return Err(unsupported("snapshot spread source")),
                        };
                        for field in source {
                            fields.insert(field.name.clone(), field);
                        }
                    }
                }
                ast::PropOrSpread::Prop(property) => {
                    let (name, expr) = match &**property {
                        ast::Prop::KeyValue(field) => {
                            (records::property_key(&field.key)?, *field.value.clone())
                        }
                        ast::Prop::Shorthand(id) => (
                            records::key(id.sym.to_string())?,
                            ast::Expr::Ident(id.clone()),
                        ),
                        _ => return Err(unsupported("snapshot spread with accessor or method")),
                    };
                    let ty = self
                        .peek_type(&expr, 0)
                        .map(Ok)
                        .unwrap_or_else(|| self.expr(&expr).map(|value| value.ty))?;
                    fields.insert(
                        name.clone(),
                        ir::RecordField {
                            name,
                            ty,
                            optional: false,
                        },
                    );
                }
            }
        }
        let ty = mixed_records::intern(
            Type::Unknown,
            fields.into_values().collect(),
            false,
            self.types,
        )?;
        let Type::Dictionary(id) = ty else {
            return Err(invalid("snapshot spread dictionary"));
        };
        let result = self.dictionary_spread_literal_mode(object, id, create_data)?;
        Ok(Some(match expected {
            Some(target @ Type::Record(_)) => {
                self.snapshot_spread_result(result, target, object.span)?
            }
            _ => result,
        }))
    }
}
