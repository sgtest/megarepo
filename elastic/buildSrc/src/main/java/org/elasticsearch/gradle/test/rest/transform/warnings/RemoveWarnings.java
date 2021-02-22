/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.gradle.test.rest.transform.warnings;

import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import org.elasticsearch.gradle.test.rest.transform.RestTestTransformByParentObject;
import org.gradle.api.tasks.Input;
import org.gradle.api.tasks.Internal;

import java.util.ArrayList;
import java.util.List;
import java.util.Set;

/**
 * A transformation to to remove any warnings that match exactly.
 * If this removes all of the warnings, this will not remove the feature from the setup and/or teardown and will leave behind an empty array
 * While it would be more technically correct to do so, the effort/complexity does not warrant it, since for the expected usage it makes
 * no difference.
 */

public class RemoveWarnings implements RestTestTransformByParentObject {

    private final Set<String> warnings;

    /**
     * @param warnings The allowed warnings to inject
     */
    public RemoveWarnings(Set<String> warnings) {
        this.warnings = warnings;
    }

    @Override
    public void transformTest(ObjectNode doNodeParent) {
        ObjectNode doNodeValue = (ObjectNode) doNodeParent.get(getKeyToFind());
        ArrayNode arrayWarnings = (ArrayNode) doNodeValue.get("warnings");
        if (arrayWarnings == null) {
            return;
        }

        List<String> keepWarnings = new ArrayList<>();
        arrayWarnings.elements().forEachRemaining(warning -> {
            String warningValue = warning.textValue();
            if (warnings.contains(warningValue) == false) {
                keepWarnings.add(warningValue);
            }

        });
        arrayWarnings.removeAll();
        keepWarnings.forEach(arrayWarnings::add);
    }

    @Override
    @Internal
    public String getKeyToFind() {
        return "do";
    }

    @Input
    public Set<String> getWarnings() {
        return warnings;
    }
}
