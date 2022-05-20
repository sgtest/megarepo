/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.health;

import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xcontent.ToXContent;
import org.elasticsearch.xcontent.XContentBuilder;
import org.elasticsearch.xcontent.XContentFactory;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

public class HealthIndicatorResultTests extends ESTestCase {
    public void testToXContent() throws Exception {
        String name = randomAlphaOfLength(10);
        String component = randomAlphaOfLength(10);
        HealthStatus status = randomFrom(HealthStatus.RED, HealthStatus.YELLOW, HealthStatus.GREEN);
        String summary = randomAlphaOfLength(20);
        String helpUrl = randomAlphaOfLength(20);
        Map<String, Object> detailsMap = new HashMap<>();
        detailsMap.put("key", "value");
        HealthIndicatorDetails details = new SimpleHealthIndicatorDetails(detailsMap);
        List<HealthIndicatorImpact> impacts = new ArrayList<>();
        int impact1Severity = randomIntBetween(1, 5);
        String impact1Description = randomAlphaOfLength(30);
        ImpactArea firstImpactArea = randomFrom(ImpactArea.values());
        impacts.add(new HealthIndicatorImpact(impact1Severity, impact1Description, List.of(firstImpactArea)));
        int impact2Severity = randomIntBetween(1, 5);
        String impact2Description = randomAlphaOfLength(30);
        ImpactArea secondImpactArea = randomFrom(ImpactArea.values());
        impacts.add(new HealthIndicatorImpact(impact2Severity, impact2Description, List.of(secondImpactArea)));
        List<UserAction> actions = new ArrayList<>();
        UserAction action1 = new UserAction(
            new UserAction.Definition(randomAlphaOfLength(10), randomAlphaOfLength(50), randomAlphaOfLength(30)),
            new ArrayList<>()
        );
        for (int i = 0; i < randomInt(10); i++) {
            action1.affectedResources().add(randomAlphaOfLength(10));
        }
        actions.add(action1);
        UserAction action2 = new UserAction(
            new UserAction.Definition(randomAlphaOfLength(10), randomAlphaOfLength(50), randomAlphaOfLength(30)),
            new ArrayList<>()
        );
        for (int i = 0; i < randomInt(10); i++) {
            action2.affectedResources().add(randomAlphaOfLength(10));
        }
        actions.add(action2);
        HealthIndicatorResult result = new HealthIndicatorResult(name, component, status, summary, helpUrl, details, impacts, actions);
        XContentBuilder builder = XContentFactory.jsonBuilder().prettyPrint();
        result.toXContent(builder, ToXContent.EMPTY_PARAMS);
        Map<String, Object> xContentMap = XContentHelper.convertToMap(BytesReference.bytes(builder), false, builder.contentType()).v2();
        assertEquals(status.xContentValue(), xContentMap.get("status"));
        assertEquals(summary, xContentMap.get("summary"));
        assertEquals(helpUrl, xContentMap.get("help_url"));
        assertEquals(detailsMap, xContentMap.get("details"));
        List<Map<String, Object>> expectedImpacts = new ArrayList<>();
        Map<String, Object> expectedImpact1 = new HashMap<>();
        expectedImpact1.put("severity", impact1Severity);
        expectedImpact1.put("description", impact1Description);
        expectedImpact1.put("impact_areas", List.of(firstImpactArea.displayValue()));
        Map<String, Object> expectedImpact2 = new HashMap<>();
        expectedImpact2.put("severity", impact2Severity);
        expectedImpact2.put("description", impact2Description);
        expectedImpact2.put("impact_areas", List.of(secondImpactArea.displayValue()));
        expectedImpacts.add(expectedImpact1);
        expectedImpacts.add(expectedImpact2);
        assertEquals(expectedImpacts, xContentMap.get("impacts"));
        List<Map<String, Object>> expectedUserActions = new ArrayList<>();
        {
            Map<String, Object> expectedAction1 = new HashMap<>();
            expectedAction1.put("message", action1.definition().message());
            expectedAction1.put("help_url", action1.definition().helpURL());
            if (action1.affectedResources().isEmpty() == false) {
                expectedAction1.put("affected_resources", action1.affectedResources());
            }
            expectedUserActions.add(expectedAction1);
        }
        {
            Map<String, Object> expectedAction2 = new HashMap<>();
            expectedAction2.put("message", action2.definition().message());
            expectedAction2.put("help_url", action2.definition().helpURL());
            if (action2.affectedResources().isEmpty() == false) {
                expectedAction2.put("affected_resources", action2.affectedResources());
            }
            expectedUserActions.add(expectedAction2);
        }
        assertEquals(expectedUserActions, xContentMap.get("user_actions"));
    }
}
