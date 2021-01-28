/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.sql.expression.function.scalar.string;

import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.tree.NodeInfo;
import org.elasticsearch.xpack.ql.tree.Source;

import java.util.List;

public class StartsWith extends org.elasticsearch.xpack.ql.expression.function.scalar.string.StartsWith {

    public StartsWith(Source source, Expression input, Expression pattern) {
        super(source, input, pattern, false);
    }

    @Override
    protected NodeInfo<? extends Expression> info() {
        return NodeInfo.create(this, StartsWith::new, input(), pattern());
    }

    @Override
    public Expression replaceChildren(List<Expression> newChildren) {
        return new StartsWith(source(), newChildren.get(0), newChildren.get(1));
    }
}
