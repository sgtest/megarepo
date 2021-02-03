/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.eql;

import org.elasticsearch.test.eql.EqlExtraSpecTestCase;

public class EqlExtraIT extends EqlExtraSpecTestCase {

    public EqlExtraIT(String query, String name, long[] eventIds) {
        super(query, name, eventIds);
    }
}
