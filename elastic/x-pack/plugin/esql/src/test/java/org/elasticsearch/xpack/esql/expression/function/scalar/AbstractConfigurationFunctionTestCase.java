/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.esql.expression.function.scalar;

import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.xpack.esql.EsqlTestUtils;
import org.elasticsearch.xpack.esql.expression.function.AbstractFunctionTestCase;
import org.elasticsearch.xpack.esql.plugin.EsqlPlugin;
import org.elasticsearch.xpack.esql.plugin.QueryPragmas;
import org.elasticsearch.xpack.esql.session.EsqlConfiguration;
import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.tree.Source;
import org.elasticsearch.xpack.ql.util.StringUtils;

import java.util.List;

import static org.elasticsearch.xpack.esql.SerializationTestUtils.assertSerialization;

public abstract class AbstractConfigurationFunctionTestCase extends AbstractFunctionTestCase {
    protected abstract Expression buildWithConfiguration(Source source, List<Expression> args, EsqlConfiguration configuration);

    @Override
    protected Expression build(Source source, List<Expression> args) {
        return buildWithConfiguration(source, args, EsqlTestUtils.TEST_CFG);
    }

    static EsqlConfiguration randomConfiguration() {
        // TODO: Randomize the query and maybe the pragmas.
        return new EsqlConfiguration(
            randomZone(),
            randomLocale(random()),
            randomBoolean() ? null : randomAlphaOfLength(randomInt(64)),
            randomBoolean() ? null : randomAlphaOfLength(randomInt(64)),
            QueryPragmas.EMPTY,
            EsqlPlugin.QUERY_RESULT_TRUNCATION_MAX_SIZE.getDefault(Settings.EMPTY),
            EsqlPlugin.QUERY_RESULT_TRUNCATION_DEFAULT_SIZE.getDefault(Settings.EMPTY),
            StringUtils.EMPTY,
            randomBoolean()
        );
    }

    public void testSerializationWithConfiguration() {
        EsqlConfiguration config = randomConfiguration();
        Expression expr = buildWithConfiguration(testCase.getSource(), testCase.getDataAsFields(), config);

        assertSerialization(expr, config);

        EsqlConfiguration differentConfig;
        do {
            differentConfig = randomConfiguration();
        } while (config.equals(differentConfig));

        Expression differentExpr = buildWithConfiguration(testCase.getSource(), testCase.getDataAsFields(), differentConfig);
        assertFalse(expr.equals(differentExpr));
    }
}
