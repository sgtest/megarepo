import argparse
import json
import os
import sys
import jsonschema
import retry
import urllib.request

CYCLONEDX_VERSION = 1.5
# json schema for CycloneDX sbom files
SCHEMA_URL = f"https://cyclonedx.org/schema/bom-{CYCLONEDX_VERSION}.schema.json"
# directory to scan for third party libraries
THIRD_PARTY_DIR = os.path.join("src", "third_party")
# platform independent prefix of third party libraries
THIRD_PARTY_LOCATION_PREFIX = "src/third_party/"
# This should only be set to true for testing to ensure the tests to not rely on the current state
# of the third party library dir.
SKIP_FILE_CHECKING = False

# Error messages used for matching in testing
UNDEFINED_THIRD_PARTY_ERROR = (
    "The following files in src/third_party do not have components defined in the sbom:"
)
FORMATTING_ERROR = (
    "file has incorrect formatting, re-run this linter with the `--format` option to fix this."
)
MISSING_PURL_CPE_ERROR = "component must include a 'purl' or 'cpe' field."
MISSING_EVIDENCE_ERROR = (
    "component must include an 'evidence.occurrences' field when the scope is required."
)
MISSING_TEAM_ERROR = "component must include a 'internal:team_responsible' property."


@retry.retry(tries=3, delay=5)
def get_schema():
    with urllib.request.urlopen(SCHEMA_URL) as schema_data:
        return json.load(schema_data)


def lint_sbom(
    input_file: str, output_file: str, third_party_libs: set, should_format: bool
) -> list:
    with open(input_file, "r", encoding="utf-8") as sbom_file:
        sbom_text = sbom_file.read()

    errors = []

    try:
        sbom = json.loads(sbom_text)
    except Exception as ex:
        errors.append(f"Failed to parse {input_file}: {str(ex)}")
        return errors

    try:
        jsonschema.validate(sbom, get_schema())
    except jsonschema.ValidationError as error:
        errors.append(f"sbom.json file did not match the CycloneDX schema from {SCHEMA_URL}")
        errors.append(error.message)
        return errors

    errors = []
    components = sbom["components"]
    for component in components:
        name = component["name"]

        def add_component_error(name: str, message: str):
            errors.append(f"Error in component {name}: {message}")

        if "scope" not in component:
            add_component_error(name, "component must include a scope.")
        elif component["scope"] != "optional":
            if "evidence" in component and "occurrences" in component["evidence"]:
                occurrences = component["evidence"]["occurrences"]
                if not occurrences:
                    add_component_error(
                        name, "'evidence.occurrences' field must include at least one location."
                    )
                for occurrence in occurrences:
                    location = occurrence["location"]

                    if not os.path.exists(location) and not SKIP_FILE_CHECKING:
                        add_component_error(name, "location does not exist in repo.")

                    if location.startswith(THIRD_PARTY_LOCATION_PREFIX):
                        lib = location[len(THIRD_PARTY_LOCATION_PREFIX) :]
                        if lib in third_party_libs:
                            third_party_libs.remove(lib)
            else:
                add_component_error(name, MISSING_EVIDENCE_ERROR)

        has_team_responsible_property = False
        if "properties" in component:
            for prop in component["properties"]:
                if prop["name"] == "internal:team_responsible":
                    has_team_responsible_property = True
                    break

        if not has_team_responsible_property:
            add_component_error(name, MISSING_TEAM_ERROR)

        if "purl" not in component and "cpe" not in component:
            add_component_error(name, MISSING_PURL_CPE_ERROR)

    if third_party_libs:
        errors.append(UNDEFINED_THIRD_PARTY_ERROR)
        for lib in third_party_libs:
            errors.append(f"    {lib}")

    formatted_sbom = json.dumps(sbom, indent=2) + "\n"
    if formatted_sbom != sbom_text:
        errors.append(f"{input_file} {FORMATTING_ERROR}")

    if should_format:
        with open(output_file, "w", encoding="utf-8") as sbom_file:
            sbom_file.write(formatted_sbom)
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--format",
        action="store_true",
        default=False,
        help="Whether to apply formatting to the output file.",
    )
    parser.add_argument(
        "--input-file", default="sbom.json", help="The input CycloneDX file to format and lint."
    )
    parser.add_argument(
        "--output-file",
        default="sbom.json",
        help="The file to output to when formatting is specified.",
    )
    args = parser.parse_args()
    should_format = args.format
    input_file = args.input_file
    output_file = args.output_file
    third_party_libs = set(
        [
            path
            for path in os.listdir(THIRD_PARTY_DIR)
            if not os.path.isfile(os.path.join(THIRD_PARTY_DIR, path))
        ]
    )
    # the only files in this dir that are not third party libs
    third_party_libs.remove("scripts")

    errors = lint_sbom(input_file, output_file, third_party_libs, should_format)

    if errors:
        print("\n".join(errors), file=sys.stderr)

    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
