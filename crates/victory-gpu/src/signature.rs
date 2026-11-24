//! Command signatures: the record layout of indirect execution. Decoded
//! into typed arguments with their byte offsets, ready for translating
//! `ExecuteIndirect` later.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::object::{
    self, COMMAND_SIGNATURE, E_INVALIDARG, HRESULT, Iid, Object, Table, This, guid,
};
use crate::resource::PAGEABLE_IIDS;

const SIGNATURE_IIDS: [Iid; 4] = [
    guid(
        0xc36a797c,
        0xec80,
        0x4f0a,
        [0x89, 0x85, 0xa7, 0xb2, 0x47, 0x50, 0x82, 0xd1],
    ),
    PAGEABLE_IIDS[0],
    PAGEABLE_IIDS[1],
    PAGEABLE_IIDS[2],
];

#[repr(C)]
pub(crate) struct Desc {
    stride: u32,
    count: u32,
    /// Each a type and up to three 32-bit fields.
    arguments: *const [u32; 4],
    nodes: u32,
}

/// One argument of an indirect record, as the signature names it. The
/// numbers are the host's `D3D12_INDIRECT_ARGUMENT_TYPE` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Argument {
    /// 0: `(vertices, instances, first vertex, first instance)`.
    Draw,
    /// 1: `(indices, instances, first index, base vertex, first instance)`.
    DrawIndexed,
    /// 2: `(x, y, z)`.
    Dispatch,
    /// 3: a vertex buffer view for `slot`.
    VertexBuffer { slot: u32 },
    /// 4: an index buffer view.
    IndexBuffer,
    /// 5: `count` 32-bit root constants, starting `dest_offset` values into
    /// the root parameter.
    Constants {
        root_parameter: u32,
        dest_offset: u32,
        count: u32,
    },
    /// 6: a root constant buffer view (a GPU address).
    ConstantBuffer { root_parameter: u32 },
    /// 7: a root shader resource view (a GPU address).
    ShaderResource { root_parameter: u32 },
    /// 8: a root unordered access view (a GPU address).
    UnorderedAccess { root_parameter: u32 },
    /// 10: `(x, y, z)` mesh shader groups.
    DispatchMesh,
}

impl Argument {
    /// Whether the argument starts work (as opposed to setting state).
    /// A record has exactly one, and it is the last.
    fn is_action(self) -> bool {
        matches!(
            self,
            Self::Draw | Self::DrawIndexed | Self::Dispatch | Self::DispatchMesh
        )
    }

    /// Bytes the argument takes in a record (wide, so that a title's huge
    /// constant count cannot wrap).
    fn size(self) -> u64 {
        match self {
            Self::Draw => 16,
            Self::DrawIndexed => 20,
            Self::Dispatch | Self::DispatchMesh => 12,
            // Address, size, stride (or format).
            Self::VertexBuffer { .. } | Self::IndexBuffer => 16,
            Self::Constants { count, .. } => 4 * u64::from(count),
            Self::ConstantBuffer { .. }
            | Self::ShaderResource { .. }
            | Self::UnorderedAccess { .. } => 8,
        }
    }

    fn decode([kind, a, b, c]: [u32; 4]) -> Result<Self, LayoutError> {
        Ok(match kind {
            0 => Self::Draw,
            1 => Self::DrawIndexed,
            2 => Self::Dispatch,
            3 => Self::VertexBuffer { slot: a },
            4 => Self::IndexBuffer,
            5 if c == 0 => return Err(LayoutError::NoConstants),
            5 => Self::Constants {
                root_parameter: a,
                dest_offset: b,
                count: c,
            },
            6 => Self::ConstantBuffer { root_parameter: a },
            7 => Self::ShaderResource { root_parameter: a },
            8 => Self::UnorderedAccess { root_parameter: a },
            10 => Self::DispatchMesh,
            // 9 is ray dispatch: its record is not decoded yet.
            other => return Err(LayoutError::UnknownType(other)),
        })
    }
}

/// Why a signature's arguments could not be decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutError {
    /// An argument type this decoder does not know.
    UnknownType(u32),
    /// A constants argument that sets no values.
    NoConstants,
    /// No argument starts work.
    NoAction,
    /// The action is not the last argument (or there is more than one).
    ActionNotLast,
    /// The stride is smaller than the arguments together.
    StrideTooSmall { stride: u32, needed: u32 },
}

/// A decoded signature: the arguments with their offsets in a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layout {
    /// Bytes from one record to the next.
    pub stride: u32,
    /// Each argument and its byte offset in the record. Arguments follow
    /// one another without padding.
    pub arguments: Vec<(Argument, u32)>,
}

impl Layout {
    /// The argument that starts work; always the last.
    #[allow(dead_code)] // for ExecuteIndirect, once it translates
    pub(crate) fn action(&self) -> Argument {
        self.arguments.last().map_or(Argument::Draw, |&(a, _)| a)
    }

    /// Bytes the arguments take in a record (at most the stride).
    #[allow(dead_code)] // for ExecuteIndirect, once it translates
    pub(crate) fn used(&self) -> u32 {
        // `decode` checked that this fits the stride, and so a `u32`.
        self.arguments
            .last()
            .map_or(0, |&(a, offset)| (u64::from(offset) + a.size()) as u32)
    }
}

/// Decodes a signature's raw arguments (a type and three fields each).
pub(crate) fn decode(stride: u32, raw: &[[u32; 4]]) -> Result<Layout, LayoutError> {
    let mut arguments = Vec::with_capacity(raw.len());
    let mut offset = 0u64;
    for (i, &entry) in raw.iter().enumerate() {
        let argument = Argument::decode(entry)?;
        if argument.is_action() != (i + 1 == raw.len()) {
            return Err(if argument.is_action() {
                LayoutError::ActionNotLast
            } else {
                LayoutError::NoAction
            });
        }
        // Kept even when too large: such a signature is refused below.
        arguments.push((argument, u32::try_from(offset).unwrap_or(u32::MAX)));
        offset += argument.size();
    }
    if arguments.is_empty() {
        return Err(LayoutError::NoAction);
    }
    if u64::from(stride) < offset {
        return Err(LayoutError::StrideTooSmall {
            stride,
            needed: u32::try_from(offset).unwrap_or(u32::MAX),
        });
    }
    Ok(Layout { stride, arguments })
}

#[repr(C)]
struct Signature {
    object: Object,
    /// `None` (with the reason logged) for a signature that does not decode.
    #[allow(dead_code)] // for ExecuteIndirect, once it translates
    layout: Option<Layout>,
}

/// Device method.
pub(crate) extern "system" fn create(
    _: This,
    desc: *const Desc,
    _root: This,
    iid: *const Iid,
    out: *mut This,
) -> HRESULT {
    // SAFETY: the title passes a description (null is tolerated).
    let Some(desc) = (unsafe { desc.as_ref() }) else {
        return E_INVALIDARG;
    };
    if desc.arguments.is_null() && desc.count > 0 {
        return E_INVALIDARG;
    }
    let arguments = if desc.count == 0 {
        Vec::new()
    } else {
        // SAFETY: the title passes `count` argument descriptions.
        unsafe { std::slice::from_raw_parts(desc.arguments, desc.count as usize) }.to_vec()
    };
    tracing::debug!(stride = desc.stride, ?arguments, "command signature");
    let layout = match decode(desc.stride, &arguments) {
        Ok(layout) => Some(layout),
        Err(error) => {
            tracing::warn!(
                ?error,
                stride = desc.stride,
                ?arguments,
                "command signature"
            );
            None
        }
    };
    let signature = Box::new(Signature {
        object: Object::new(&FUNCTIONS),
        layout,
    });
    object::hand_out(COMMAND_SIGNATURE, signature, iid, &SIGNATURE_IIDS, out)
}

extern "system" fn query(this: This, iid: *const Iid, out: *mut This) -> HRESULT {
    let found = object::is(iid, &SIGNATURE_IIDS).then_some(this);
    object::answer(COMMAND_SIGNATURE, iid, found, out)
}
extern "system" fn release(this: This) -> u32 {
    object::release_boxed::<Signature>(this)
}

/// IUnknown 3, object 4, device child 1.
static FUNCTIONS: Table<24> = {
    let mut f = object::d3d12_object(object::missing_table::<COMMAND_SIGNATURE, 24>());
    f[0] = query as *const ();
    f[2] = release as *const ();
    f[7] = object::parent_device as *const (); // 0x38
    Table(f)
};

#[cfg(test)]
mod tests {
    use super::*;

    const DRAW: [u32; 4] = [0, 0, 0, 0];
    const DRAW_INDEXED: [u32; 4] = [1, 0, 0, 0];
    const DISPATCH: [u32; 4] = [2, 0, 0, 0];
    const MESH: [u32; 4] = [10, 0, 0, 0];

    #[test]
    fn a_bare_action_is_one_record() {
        let layout = decode(16, &[DRAW]).unwrap();
        assert_eq!(layout.arguments, [(Argument::Draw, 0)]);
        assert_eq!((layout.stride, layout.used()), (16, 16));
        assert_eq!(layout.action(), Argument::Draw);
        assert_eq!(decode(20, &[DRAW_INDEXED]).unwrap().used(), 20);
        assert_eq!(decode(12, &[DISPATCH]).unwrap().used(), 12);
        assert_eq!(decode(12, &[MESH]).unwrap().used(), 12);
    }

    #[test]
    fn state_comes_first_and_offsets_add_up() {
        // Root constants, a constant buffer address, then a dispatch.
        let raw = [[5, 2, 1, 4], [6, 3, 0, 0], DISPATCH];
        let layout = decode(36, &raw).unwrap();
        assert_eq!(
            layout.arguments,
            [
                (
                    Argument::Constants {
                        root_parameter: 2,
                        dest_offset: 1,
                        count: 4
                    },
                    0
                ),
                (Argument::ConstantBuffer { root_parameter: 3 }, 16),
                (Argument::Dispatch, 24),
            ]
        );
        assert_eq!(layout.used(), 36); // 16 + 8 + 12
        // The stride must cover the arguments.
        assert_eq!(
            decode(32, &raw),
            Err(LayoutError::StrideTooSmall {
                stride: 32,
                needed: 36
            })
        );
    }

    #[test]
    fn views_have_their_sizes() {
        let raw = [[3, 1, 0, 0], [4, 0, 0, 0], [7, 0, 0, 0], [8, 1, 0, 0], DRAW];
        let layout = decode(64, &raw).unwrap();
        let offsets: Vec<u32> = layout.arguments.iter().map(|&(_, o)| o).collect();
        assert_eq!(offsets, [0, 16, 32, 40, 48]);
        assert_eq!(layout.used(), 64);
    }

    #[test]
    fn a_larger_stride_is_padding() {
        assert_eq!(decode(256, &[DRAW]).unwrap().used(), 16);
    }

    #[test]
    fn the_action_must_be_alone_and_last() {
        assert_eq!(decode(64, &[]), Err(LayoutError::NoAction));
        assert_eq!(decode(64, &[[6, 0, 0, 0]]), Err(LayoutError::NoAction));
        assert_eq!(
            decode(64, &[DRAW, [6, 0, 0, 0]]),
            Err(LayoutError::ActionNotLast)
        );
        assert_eq!(
            decode(64, &[DRAW, DISPATCH]),
            Err(LayoutError::ActionNotLast)
        );
    }

    #[test]
    fn bad_arguments_are_refused() {
        assert_eq!(
            decode(64, &[[5, 0, 0, 0], DRAW]),
            Err(LayoutError::NoConstants)
        );
        // Ray dispatches (9) and anything unknown.
        assert_eq!(
            decode(64, &[[9, 0, 0, 0]]),
            Err(LayoutError::UnknownType(9))
        );
        assert_eq!(
            decode(64, &[[77, 0, 0, 0]]),
            Err(LayoutError::UnknownType(77))
        );
    }

    #[test]
    fn sizes_do_not_overflow() {
        let raw = [[5, 0, 0, 0x4000_0000], [5, 0, 0, 0x4000_0000], DRAW];
        // 2 * 4 GiB of constants: more than any stride can hold.
        assert_eq!(
            decode(u32::MAX, &raw),
            Err(LayoutError::StrideTooSmall {
                stride: u32::MAX,
                needed: u32::MAX
            })
        );
    }
}
