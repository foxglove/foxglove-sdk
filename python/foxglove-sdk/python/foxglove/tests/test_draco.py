"""Tests for the Draco point-cloud compression option classes.

These live in their own always-run suite (not test_remote_access.py, which is skipped on
builds without the remote-access feature) because DracoEncodeOptions and DracoMethod are
registered on all non-wasm wheels, including lean ones.
"""

import typing

import pytest
from foxglove import DracoEncodeOptions, DracoMethod


def test_draco_encode_options_defaults() -> None:
    options = DracoEncodeOptions()
    assert options.method == DracoMethod.KdTree
    assert options.quantization_bits == 12

    options = DracoEncodeOptions(method=DracoMethod.KdTree, quantization_bits=10)
    assert options.method == DracoMethod.KdTree
    assert options.quantization_bits == 10


@typing.no_type_check
def test_draco_encode_options_rejects_non_u8_quantization_bits() -> None:
    with pytest.raises(OverflowError):
        # quantization_bits must fit in a u8
        DracoEncodeOptions(quantization_bits=256)
