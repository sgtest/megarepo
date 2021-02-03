/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.watcher;

import org.elasticsearch.painless.spi.PainlessExtension;
import org.elasticsearch.painless.spi.Whitelist;
import org.elasticsearch.painless.spi.WhitelistLoader;
import org.elasticsearch.script.ScriptContext;
import org.elasticsearch.xpack.watcher.condition.WatcherConditionScript;
import org.elasticsearch.xpack.watcher.transform.script.WatcherTransformScript;

import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

public class WatcherPainlessExtension implements PainlessExtension {

    private static final Whitelist WHITELIST =
        WhitelistLoader.loadFromResourceFiles(WatcherPainlessExtension.class, "painless_whitelist.txt");

    @Override
    public Map<ScriptContext<?>, List<Whitelist>> getContextWhitelists() {
        Map<ScriptContext<?>, List<Whitelist>> contextWhiltelists = new HashMap<>();
        contextWhiltelists.put(WatcherConditionScript.CONTEXT, Collections.singletonList(WHITELIST));
        contextWhiltelists.put(WatcherTransformScript.CONTEXT, Collections.singletonList(WHITELIST));
        return Collections.unmodifiableMap(contextWhiltelists);
    }
}
