"""
Provides a temporary list of patches.

These are created temporarily and should be moved to the first copybara workflow as a public or an
internal patch during the next triton integration process.
"""

temporary_patch_list = [
    "//third_party/triton/temporary:fp8_splat_partial_revert.patch",
    "//third_party/triton/temporary:local_alloc_lowering_fix.patch",
]
