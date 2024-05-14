"""
Provides a temporary list of patches.

These are created temporarily and should be moved to the first copybara workflow as a public or an
internal patch during the next triton integration process.
"""

temporary_patch_list = [
    "//third_party/triton/temporary:pipelining.patch",
    "//third_party/triton/temporary:support_ceil_op.patch",
    "//third_party/triton/temporary:mma_limit_pred.patch",
]
