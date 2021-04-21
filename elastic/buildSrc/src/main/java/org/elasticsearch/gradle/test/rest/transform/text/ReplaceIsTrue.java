/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.gradle.test.rest.transform.text;

import com.fasterxml.jackson.databind.node.TextNode;

public class ReplaceIsTrue extends ReplaceTextual {
    public ReplaceIsTrue(String valueToBeReplaced, TextNode replacementNode) {
        super("is_true", valueToBeReplaced, replacementNode);
    }
}
