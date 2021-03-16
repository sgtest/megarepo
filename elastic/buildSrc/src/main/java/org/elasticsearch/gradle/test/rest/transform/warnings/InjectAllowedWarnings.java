/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.gradle.test.rest.transform.warnings;

import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.JsonNodeFactory;
import com.fasterxml.jackson.databind.node.ObjectNode;
import org.elasticsearch.gradle.test.rest.transform.RestTestTransformByParentObject;
import org.elasticsearch.gradle.test.rest.transform.feature.FeatureInjector;
import org.gradle.api.tasks.Input;
import org.gradle.api.tasks.Internal;

import java.util.List;

/**
 * A transformation to inject an allowed warning.
 */
public class InjectAllowedWarnings extends FeatureInjector implements RestTestTransformByParentObject {

    private static JsonNodeFactory jsonNodeFactory = JsonNodeFactory.withExactBigDecimals(false);

    private final List<String> allowedWarnings;
    private final boolean isRegex;

    /**
     * @param allowedWarnings The allowed warnings to inject
     */
    public InjectAllowedWarnings(List<String> allowedWarnings) {
        this(false, allowedWarnings);
    }

    /**
     * @param isRegex true if should inject the regex variant of allowed warnings
     * @param allowedWarnings The allowed warnings to inject
     */
    public InjectAllowedWarnings(boolean isRegex, List<String> allowedWarnings) {
        this.isRegex = isRegex;
        this.allowedWarnings = allowedWarnings;
    }

    @Override
    public void transformTest(ObjectNode doNodeParent) {
        ObjectNode doNodeValue = (ObjectNode) doNodeParent.get(getKeyToFind());
        ArrayNode arrayWarnings = (ArrayNode) doNodeValue.get(getSkipFeatureName());
        if (arrayWarnings == null) {
            arrayWarnings = new ArrayNode(jsonNodeFactory);
            doNodeValue.set(getSkipFeatureName(), arrayWarnings);
        }
        allowedWarnings.forEach(arrayWarnings::add);
    }

    @Override
    @Internal
    public String getKeyToFind() {
        return "do";
    }

    @Override
    @Input
    public String getSkipFeatureName() {
        return isRegex ? "allowed_warnings_regex" : "allowed_warnings";
    }

    @Input
    public List<String> getAllowedWarnings() {
        return allowedWarnings;
    }
}
