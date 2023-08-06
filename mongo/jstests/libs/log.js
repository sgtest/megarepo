
// Yields every logline that contains the specified fields. The regex escape function used here is
// drawn from the following:
// https://stackoverflow.com/questions/3561493/is-there-a-regexp-escape-function-in-javascript
// https://github.com/ljharb/regexp.escape
export function* findMatchingLogLines(logLines, fields, ignoreFields) {
    ignoreFields = ignoreFields || [];
    function escapeRegex(input) {
        return (typeof input === "string" ? input.replace(/[\^\$\\\.\*\+\?\(\)\[\]\{\}]/g, '\\$&')
                                          : input);
    }

    function lineMatches(line, fields, ignoreFields) {
        const fieldNames =
            Object.keys(fields).filter((fieldName) => !ignoreFields.includes(fieldName));
        return fieldNames.every((fieldName) => {
            const fieldValue = fields[fieldName];
            let regex;
            const booleanFields = [
                'cursorExhausted',
                'upsert',
                'hasSortStage',
                'usedDisk',
                'cursorExhausted',
                'cursorExhausted'
            ];

            // Command is a special case since it is the first arg of the message, not a
            // separate field
            if (fieldName === "command") {
                let commandName = fieldValue;

                // These commands can be sent camelCase or lower case but shell sends them lower
                // case
                if (fieldValue === "findAndModify" || fieldValue === "mapReduce") {
                    commandName = fieldValue.toLowerCase();
                }

                regex = `"command":{"${commandName}`;
            } else if (fieldName === "insert" && fieldValue.indexOf("|") != -1) {
                // Match new and legacy insert
                regex = `("insert","ns":"(${fieldValue})"|("insert":"(${fieldValue})"))`;
            } else if (booleanFields.find(f => f === fieldName) && fieldValue == 1) {
                regex = `"${fieldName}":true`;
            } else {
                regex = "\"" + escapeRegex(fieldName) + "\":(" +
                    escapeRegex(checkLog.formatAsJsonLogLine(fieldValue)) + "|" +
                    escapeRegex(checkLog.formatAsJsonLogLine(fieldValue, true)) + ")";
            }
            const match = line.match(regex);
            return match && match[0];
        });
    }

    for (const line of logLines) {
        if (lineMatches(line, fields, ignoreFields)) {
            yield line;
        }
    }
}

// Finds and returns a logline containing all the specified fields, or null if no such logline
// was found.
export function findMatchingLogLine(logLines, fields, ignoreFields) {
    for (const line of findMatchingLogLines(logLines, fields, ignoreFields)) {
        return line;
    }
    return null;
}
