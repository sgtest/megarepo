/**
 * Resolves the command name for the given 'cmdObj'.
 */
export function getCommandName(cmdObj) {
    return Object.keys(cmdObj)[0];
}

/**
 * Returns the inner command if 'cmdObj' represents an explain command, or simply 'cmdObj'
 * otherwise.
 */
export function getInnerCommand(cmdObj) {
    const isExplain = "explain" in cmdObj;
    if (!isExplain) {
        return cmdObj;
    }

    if (typeof cmdObj.explain === "object") {
        return cmdObj.explain;
    }

    const {explain, ...cmdWithoutExplain} = cmdObj;
    return cmdWithoutExplain;
}

/**
 *  Returns the explain command object for the given 'cmdObj'.
 */
export function getExplainCommand(cmdObj) {
    const isAggregateCmd = getCommandName(cmdObj) === "aggregate";
    return isAggregateCmd ? {explain: {...cmdObj, cursor: {}}} : {explain: cmdObj};
}

/**
 * Resolves the collection name for the given 'cmdObj'. If the command targets a view, then this
 * will return the underlying collection's name. Returns 'undefined' if the collection does not
 * exist.
 */
export function getCollectionName(cmdObj) {
    const name = cmdObj[getCommandName(cmdObj)];
    const collInfos = db.getCollectionInfos({name});
    if (!collInfos || collInfos.length === 0) {
        return undefined;
    }
    return collInfos[0].options.viewOn || name;
}

export function isSystemCollectionName(collectionName) {
    return collectionName.startsWith("system.");
}

export function isInternalDbName(dbName) {
    return ["admin", "local", "config"].includes(dbName);
}
