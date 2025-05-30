use std::collections::VecDeque;
use std::io::{Read, Seek};

use polars_error::{PolarsResult, polars_ensure, polars_err};

use super::super::super::IpcField;
use super::super::deserialize::{read, skip};
use super::super::read_basic::*;
use super::super::{Compression, Dictionaries, IpcBuffer, Node, Version};
use crate::array::FixedSizeListArray;
use crate::datatypes::ArrowDataType;
use crate::io::ipc::read::array::try_get_field_node;

#[allow(clippy::too_many_arguments)]
pub fn read_fixed_size_list<R: Read + Seek>(
    field_nodes: &mut VecDeque<Node>,
    variadic_buffer_counts: &mut VecDeque<usize>,
    dtype: ArrowDataType,
    ipc_field: &IpcField,
    buffers: &mut VecDeque<IpcBuffer>,
    reader: &mut R,
    dictionaries: &Dictionaries,
    block_offset: u64,
    is_little_endian: bool,
    compression: Option<Compression>,
    limit: Option<usize>,
    version: Version,
    scratch: &mut Vec<u8>,
) -> PolarsResult<FixedSizeListArray> {
    let field_node = try_get_field_node(field_nodes, &dtype)?;

    let validity = read_validity(
        buffers,
        field_node,
        reader,
        block_offset,
        is_little_endian,
        compression,
        limit,
        scratch,
    )?;

    let (field, size) = FixedSizeListArray::get_child_and_size(&dtype);
    polars_ensure!(size > 0, nyi = "Cannot read zero sized arrays from IPC");

    let limit = limit.map(|x| x.saturating_mul(size));

    let values = read(
        field_nodes,
        variadic_buffer_counts,
        field,
        &ipc_field.fields[0],
        buffers,
        reader,
        dictionaries,
        block_offset,
        is_little_endian,
        compression,
        limit,
        version,
        scratch,
    )?;
    FixedSizeListArray::try_new(dtype, values.len() / size, values, validity)
}

pub fn skip_fixed_size_list(
    field_nodes: &mut VecDeque<Node>,
    dtype: &ArrowDataType,
    buffers: &mut VecDeque<IpcBuffer>,
    variadic_buffer_counts: &mut VecDeque<usize>,
) -> PolarsResult<()> {
    let _ = field_nodes.pop_front().ok_or_else(|| {
        polars_err!(oos =
            "IPC: unable to fetch the field for fixed-size list. The file or stream is corrupted."
        )
    })?;

    let _ = buffers
        .pop_front()
        .ok_or_else(|| polars_err!(oos = "IPC: missing validity buffer."))?;

    let (field, _) = FixedSizeListArray::get_child_and_size(dtype);

    skip(field_nodes, field.dtype(), buffers, variadic_buffer_counts)
}
