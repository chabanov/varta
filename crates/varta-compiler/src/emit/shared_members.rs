//! Union members that share a value tag (v37 `language_core`).
//!
//! Records and class instances share the record tag of a union cell, and all
//! arrays share the array tag. A union with several such members tells them
//! apart by the payload itself: a record or instance by its layout
//! descriptor (any layout of the member's shape, or of a class and its
//! subclasses), an array by its element kind, which lowering requires to
//! differ between the array members of such a union. Unions with at most one
//! member per tag keep the tag-only test, so earlier profiles (which never
//! build such unions) emit the same code.
use super::*;

#[derive(Default)]
pub(super) struct SharedMembers {
    /// Unions with more than one record-tag member or more than one array.
    unions: BTreeSet<u32>,
    /// The record shapes of each record or class member of those unions.
    shapes: BTreeMap<Type, Vec<u32>>,
}

pub(super) fn shared_members(types: &ir::ValueTypes) -> SharedMembers {
    let mut shared = SharedMembers::default();
    if !types.profile.language_core() {
        return shared;
    }
    for (id, members) in types.unions.iter().enumerate() {
        let records = members
            .iter()
            .filter(|ty| matches!(ty, Type::Record(_) | Type::Class(_)))
            .count();
        let arrays = members
            .iter()
            .filter(|ty| matches!(ty, Type::Array(_)))
            .count();
        if records < 2 && arrays < 2 {
            continue;
        }
        shared.unions.insert(id as u32);
        for member in members {
            match *member {
                Type::Record(shape) => {
                    shared.shapes.insert(*member, vec![shape]);
                }
                Type::Class(class) => {
                    shared
                        .shapes
                        .insert(*member, types.classes.family_shapes(class));
                }
                _ => {}
            }
        }
    }
    shared
}

impl Emitter<'_> {
    /// After the tag test of `member` in a cell of `union` (cell pointer in
    /// the temporary, the tag result on the stack): the payload test that
    /// tells `member` from the other members with the same tag.
    pub(super) fn shared_member(&mut self, union: Type, member: Type) -> Result<()> {
        let (Type::Union(id) | Type::ReferenceUnion(id)) = union else {
            return Ok(());
        };
        if !self.shared.unions.contains(&id) {
            return Ok(());
        }
        let test: Vec<I<'static>> = match member {
            Type::Record(_) | Type::Class(_) => {
                let shapes = self.shared.shapes.get(&member).cloned().ok_or_else(|| {
                    CompilerError::Internal("missing shared member shapes".into())
                })?;
                let mut offsets: Vec<u32> = self
                    .layouts
                    .iter()
                    .filter(|(key, _)| shapes.contains(&key.0))
                    .map(|(_, offset)| *offset)
                    .collect();
                offsets.sort_unstable();
                offsets.dedup();
                // The descriptor offset of the payload's first word.
                let mut test = vec![
                    I::LocalGet(self.temporary),
                    I::I64Load(records::field(0)),
                    I::I32WrapI64,
                    I::I32Load(strings::WORD),
                    I::LocalSet(self.temporary),
                    I::I32Const(0),
                ];
                for offset in offsets {
                    test.extend([
                        I::LocalGet(self.temporary),
                        I::I32Const(offset as i32),
                        I::I32Eq,
                        I::I32Or,
                    ]);
                }
                test
            }
            Type::Array(id) => {
                let kind = self.arrays[id as usize]
                    .profile_kind(self.profile)
                    .ok_or_else(|| CompilerError::Internal("missing array element kind".into()))?;
                vec![
                    I::LocalGet(self.temporary),
                    I::I64Load(records::field(0)),
                    I::I32WrapI64,
                    I::I32Load(strings::WORD),
                    I::I32Const(kind as i32),
                    I::I32Eq,
                ]
            }
            _ => return Ok(()),
        };
        // Only a matching tag proves the payload is a record or an array.
        self.code.push(I::If(BlockType::Result(ValType::I32)));
        self.code.extend(test);
        self.code.extend([I::Else, I::I32Const(0), I::End]);
        Ok(())
    }
}
