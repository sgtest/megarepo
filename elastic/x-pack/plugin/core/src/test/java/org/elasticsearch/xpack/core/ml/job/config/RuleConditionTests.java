/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.job.config;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.common.io.stream.Writeable.Reader;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.test.AbstractSerializingTestCase;
import org.elasticsearch.xpack.core.ml.job.messages.Messages;

public class RuleConditionTests extends AbstractSerializingTestCase<RuleCondition> {

    @Override
    protected RuleCondition createTestInstance() {
        Condition condition = null;
        String fieldName = null;
        String valueFilter = null;
        String fieldValue = null;
        RuleConditionType type = randomFrom(RuleConditionType.values());
        if (type.isCategorical()) {
            valueFilter = randomAlphaOfLengthBetween(1, 20);
            if (randomBoolean()) {
                fieldName = randomAlphaOfLengthBetween(1, 20);
            }
        } else {
            // no need to randomize, it is properly randomly tested in
            // ConditionTest
            condition = new Condition(Operator.LT, Long.toString(randomLong()));
            if (randomBoolean()) {
                fieldName = randomAlphaOfLengthBetween(1, 20);
                fieldValue = randomAlphaOfLengthBetween(1, 20);
            }
        }
        return new RuleCondition(type, fieldName, fieldValue, condition, valueFilter);
    }

    @Override
    protected Reader<RuleCondition> instanceReader() {
        return RuleCondition::new;
    }

    @Override
    protected RuleCondition doParseInstance(XContentParser parser) {
        return RuleCondition.CONFIG_PARSER.apply(parser, null);
    }

    public void testConstructor() {
        RuleCondition condition = new RuleCondition(RuleConditionType.CATEGORICAL, null, null, null, "valueFilter");
        assertEquals(RuleConditionType.CATEGORICAL, condition.getType());
        assertNull(condition.getFieldName());
        assertNull(condition.getFieldValue());
        assertNull(condition.getCondition());
    }

    public void testEqualsGivenSameObject() {
        RuleCondition condition = new RuleCondition(RuleConditionType.CATEGORICAL, null, null, null, "valueFilter");
        assertTrue(condition.equals(condition));
    }

    public void testEqualsGivenString() {
        assertFalse(new RuleCondition(RuleConditionType.CATEGORICAL, null, null, null, "filter").equals("a string"));
    }

    public void testEqualsGivenDifferentType() {
        RuleCondition condition1 = createFullyPopulated();
        RuleCondition condition2 = new RuleCondition(RuleConditionType.CATEGORICAL, null, null, null, "valueFilter");
        assertFalse(condition1.equals(condition2));
        assertFalse(condition2.equals(condition1));
    }

    public void testEqualsGivenDifferentFieldName() {
        RuleCondition condition1 = createFullyPopulated();
        RuleCondition condition2 = new RuleCondition(RuleConditionType.NUMERICAL_ACTUAL, "metricNameaaa", "cpu",
                new Condition(Operator.LT, "5"), null);
        assertFalse(condition1.equals(condition2));
        assertFalse(condition2.equals(condition1));
    }

    public void testEqualsGivenDifferentFieldValue() {
        RuleCondition condition1 = createFullyPopulated();
        RuleCondition condition2 = new RuleCondition(RuleConditionType.NUMERICAL_ACTUAL, "metricName", "cpuaaa",
                new Condition(Operator.LT, "5"), null);
        assertFalse(condition1.equals(condition2));
        assertFalse(condition2.equals(condition1));
    }

    public void testEqualsGivenDifferentCondition() {
        RuleCondition condition1 = createFullyPopulated();
        RuleCondition condition2 = new RuleCondition(RuleConditionType.NUMERICAL_ACTUAL, "metricName", "cpu",
                new Condition(Operator.GT, "5"), null);
        assertFalse(condition1.equals(condition2));
        assertFalse(condition2.equals(condition1));
    }

    public void testEqualsGivenDifferentValueFilter() {
        RuleCondition condition1 = new RuleCondition(RuleConditionType.CATEGORICAL, null, null, null, "myFilter");
        RuleCondition condition2 = new RuleCondition(RuleConditionType.CATEGORICAL, null, null, null, "myFilteraaa");
        assertFalse(condition1.equals(condition2));
        assertFalse(condition2.equals(condition1));
    }

    private static RuleCondition createFullyPopulated() {
        return new RuleCondition(RuleConditionType.NUMERICAL_ACTUAL, "metricName", "cpu", new Condition(Operator.LT, "5"), null);
    }

    public void testVerify_GivenCategoricalWithCondition() {
        Condition condition = new Condition(Operator.MATCH, "text");
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
                () -> new RuleCondition(RuleConditionType.CATEGORICAL, null, null, condition, null));
        assertEquals("Invalid detector rule: a categorical rule_condition does not support condition", e.getMessage());
    }

    public void testVerify_GivenCategoricalWithFieldValue() {
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
                () -> new RuleCondition(RuleConditionType.CATEGORICAL, "metric", "CPU", null, null));
        assertEquals("Invalid detector rule: a categorical rule_condition does not support field_value", e.getMessage());
    }

    public void testVerify_GivenCategoricalWithoutFilterId() {
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
                () -> new RuleCondition(RuleConditionType.CATEGORICAL, null, null, null, null));
        assertEquals("Invalid detector rule: a categorical rule_condition requires filter_id to be set", e.getMessage());
    }

    public void testVerify_GivenNumericalActualWithFilterId() {
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
                () -> new RuleCondition(RuleConditionType.NUMERICAL_ACTUAL, null, null, null, "myFilter"));
        assertEquals("Invalid detector rule: a numerical rule_condition does not support filter_id", e.getMessage());
    }

    public void testVerify_GivenNumericalActualWithoutCondition() {
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
                () -> new RuleCondition(RuleConditionType.NUMERICAL_ACTUAL, null, null, null, null));
        assertEquals("Invalid detector rule: a numerical rule_condition requires condition to be set", e.getMessage());
    }

    public void testVerify_GivenNumericalActualWithFieldNameButNoFieldValue() {
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
                () -> new RuleCondition(RuleConditionType.NUMERICAL_ACTUAL, "metric", null, new Condition(Operator.LT, "5"), null));
        assertEquals("Invalid detector rule: a numerical rule_condition with field_name requires that field_value is set", e.getMessage());
    }

    public void testVerify_GivenNumericalTypicalWithFilterId() {
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
                () -> new RuleCondition(RuleConditionType.NUMERICAL_ACTUAL, null, null, null, "myFilter"));
        assertEquals("Invalid detector rule: a numerical rule_condition does not support filter_id", e.getMessage());
    }

    public void testVerify_GivenNumericalTypicalWithoutCondition() {
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
                () -> new RuleCondition(RuleConditionType.NUMERICAL_ACTUAL, null, null, null, null));
        assertEquals("Invalid detector rule: a numerical rule_condition requires condition to be set", e.getMessage());
    }

    public void testVerify_GivenNumericalDiffAbsWithFilterId() {
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
                () -> new RuleCondition(RuleConditionType.NUMERICAL_DIFF_ABS, null, null, null, "myFilter"));
        assertEquals("Invalid detector rule: a numerical rule_condition does not support filter_id", e.getMessage());
    }

    public void testVerify_GivenNumericalDiffAbsWithoutCondition() {
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
                () -> new RuleCondition(RuleConditionType.NUMERICAL_DIFF_ABS, null, null, null, null));
        assertEquals("Invalid detector rule: a numerical rule_condition requires condition to be set", e.getMessage());
    }

    public void testVerify_GivenFieldValueWithoutFieldName() {
        Condition condition = new Condition(Operator.LTE, "5");
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
                () -> new RuleCondition(RuleConditionType.NUMERICAL_DIFF_ABS, null, "foo", condition, null));
        assertEquals("Invalid detector rule: missing field_name in rule_condition where field_value 'foo' is set", e.getMessage());
    }

    public void testVerify_GivenNumericalAndOperatorEquals() {
        Condition condition = new Condition(Operator.EQ, "5");
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
                () -> new RuleCondition(RuleConditionType.NUMERICAL_ACTUAL, null, null, condition, null));
        assertEquals("Invalid detector rule: operator 'eq' is not allowed", e.getMessage());
    }

    public void testVerify_GivenNumericalAndOperatorMatch() {
        Condition condition = new Condition(Operator.MATCH, "aaa");
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
                () -> new RuleCondition(RuleConditionType.NUMERICAL_ACTUAL, null, null, condition, null));
        assertEquals("Invalid detector rule: operator 'match' is not allowed", e.getMessage());
    }

    public void testVerify_GivenDetectionRuleWithInvalidCondition() {
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
                () -> new RuleCondition(RuleConditionType.NUMERICAL_ACTUAL, "metricName", "CPU", new Condition(Operator.LT, "invalid"),
                        null));
        assertEquals(Messages.getMessage(Messages.JOB_CONFIG_CONDITION_INVALID_VALUE_NUMBER, "invalid"), e.getMessage());
    }

    public void testVerify_GivenValidCategorical() {
        // no validation error:
        new RuleCondition(RuleConditionType.CATEGORICAL, "metric", null, null, "myFilter");
        new RuleCondition(RuleConditionType.CATEGORICAL_COMPLEMENT, "metric", null, null, "myFilter");
    }

    public void testVerify_GivenValidNumericalActual() {
        // no validation error:
        new RuleCondition(RuleConditionType.NUMERICAL_ACTUAL, "metric", "cpu", new Condition(Operator.GT, "5"), null);
    }

    public void testVerify_GivenValidNumericalTypical() {
        // no validation error:
        new RuleCondition(RuleConditionType.NUMERICAL_ACTUAL, "metric", "cpu", new Condition(Operator.GTE, "5"), null);
    }

    public void testVerify_GivenValidNumericalDiffAbs() {
        // no validation error:
        new RuleCondition(RuleConditionType.NUMERICAL_DIFF_ABS, "metric", "cpu", new Condition(Operator.LT, "5"), null);
    }

    public void testCreateTimeBased() {
        RuleCondition timeBased = RuleCondition.createTime(Operator.GTE, 100L);
        assertEquals(RuleConditionType.TIME, timeBased.getType());
        assertEquals(Operator.GTE, timeBased.getCondition().getOperator());
        assertEquals("100", timeBased.getCondition().getValue());
        assertNull(timeBased.getFieldName());
        assertNull(timeBased.getFieldValue());
        assertNull(timeBased.getFilterId());
    }

    public void testCreateTimeBased_GivenOperatorMatch() {
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
                () -> RuleCondition.createTime(Operator.MATCH, 100L));
        assertEquals("Invalid detector rule: operator 'match' is not allowed", e.getMessage());
    }

    public void testCreateNumerical() {
        RuleCondition ruleCondition = RuleCondition.createNumerical(RuleConditionType.NUMERICAL_ACTUAL, "foo", "bar",
                new Condition(Operator.GTE, "100"));
        assertEquals(RuleConditionType.NUMERICAL_ACTUAL, ruleCondition.getType());
        assertEquals(Operator.GTE, ruleCondition.getCondition().getOperator());
        assertEquals("100", ruleCondition.getCondition().getValue());
        assertEquals("foo", ruleCondition.getFieldName());
        assertEquals("bar", ruleCondition.getFieldValue());
        assertNull(ruleCondition.getFilterId());
    }
}
