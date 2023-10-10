load("@aspect_bazel_lib//lib:directory_path.bzl", "make_directory_path")
load("@aspect_bazel_lib//lib:copy_to_directory.bzl", "copy_to_directory")
load("@aspect_bazel_lib//lib:write_source_files.bzl", "write_source_files")
load("@aspect_bazel_lib//lib:utils.bzl", "file_exists")
load("@bazel_skylib//lib:paths.bzl", "paths")

def write_generated_to_source_files(name, target, output_files, verbose_copy=False, **kwargs):
    for dest, orig in output_files.items():
        if dest == orig:
            fail("{} and {} must differ so we can detect source files needing to be regenerated".format(dest, orig))

    # First we copy to a directory all outputs from the target, so we can refer to them
    # individually without circular deps.
    copy_to_directory(
        name = name + "_copy",
        srcs = [target],
    )

    # Write back the explicitly selected outputs to the source tree.
    write_source_files(
        name = name,
        files = {
            dest: make_directory_path(
                orig + "_directory_path",
                name + "_copy",
                orig,
            )
            for dest, orig in output_files.items()
        },
        suggested_update_target = "//dev:write_all_generated",
        visibility = ["//visibility:public"],
        **kwargs,
    )
