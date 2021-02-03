/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.security.operator;

import org.elasticsearch.ElasticsearchSecurityException;
import org.elasticsearch.action.admin.cluster.configuration.ClearVotingConfigExclusionsAction;
import org.elasticsearch.action.admin.cluster.configuration.ClearVotingConfigExclusionsRequest;
import org.elasticsearch.client.Client;
import org.elasticsearch.common.settings.SecureString;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.test.SecuritySingleNodeTestCase;
import org.elasticsearch.xpack.core.security.action.user.GetUsersAction;
import org.elasticsearch.xpack.core.security.action.user.GetUsersRequest;

import java.util.Map;

import static org.elasticsearch.test.SecuritySettingsSource.TEST_PASSWORD_HASHED;
import static org.elasticsearch.test.SecuritySettingsSourceField.TEST_PASSWORD;
import static org.elasticsearch.xpack.core.security.authc.support.UsernamePasswordToken.basicAuthHeaderValue;
import static org.hamcrest.Matchers.containsString;

public class OperatorPrivilegesSingleNodeTests extends SecuritySingleNodeTestCase {

    private static final String OPERATOR_USER_NAME = "test_operator";

    @Override
    protected String configUsers() {
        return super.configUsers()
            + OPERATOR_USER_NAME + ":" + TEST_PASSWORD_HASHED + "\n";
    }

    @Override
    protected String configRoles() {
        return super.configRoles()
            + "limited_operator:\n"
            + "  cluster:\n"
            + "    - 'cluster:admin/voting_config/clear_exclusions'\n"
            + "    - 'monitor'\n";
    }

    @Override
    protected String configUsersRoles() {
        return super.configUsersRoles()
            + "limited_operator:" + OPERATOR_USER_NAME + "\n";
    }

    @Override
    protected String configOperatorUsers() {
        return super.configOperatorUsers()
            + "operator:\n"
            + "  - usernames: ['" + OPERATOR_USER_NAME + "']\n";
    }

    @Override
    protected Settings nodeSettings() {
        Settings.Builder builder = Settings.builder().put(super.nodeSettings());
        // Ensure the new settings can be configured
        builder.put("xpack.security.operator_privileges.enabled", "true");
        return builder.build();
    }

    public void testOutcomeOfSuperuserPerformingOperatorOnlyActionWillDependOnWhetherFeatureIsEnabled() {
        final Client client = client();
        final ClearVotingConfigExclusionsRequest clearVotingConfigExclusionsRequest = new ClearVotingConfigExclusionsRequest();
        final ElasticsearchSecurityException e = expectThrows(ElasticsearchSecurityException.class,
            () -> client.execute(ClearVotingConfigExclusionsAction.INSTANCE, clearVotingConfigExclusionsRequest).actionGet());
        assertThat(e.getCause().getMessage(), containsString("Operator privileges are required for action"));
    }

    public void testOperatorUserWillSucceedToCallOperatorOnlyAction() {
        final Client client = client().filterWithHeader(Map.of(
            "Authorization",
            basicAuthHeaderValue(OPERATOR_USER_NAME, new SecureString(TEST_PASSWORD.toCharArray()))));
        final ClearVotingConfigExclusionsRequest clearVotingConfigExclusionsRequest = new ClearVotingConfigExclusionsRequest();
        client.execute(ClearVotingConfigExclusionsAction.INSTANCE, clearVotingConfigExclusionsRequest).actionGet();
    }

    public void testOperatorUserIsStillSubjectToRoleLimits() {
        final Client client = client().filterWithHeader(Map.of(
            "Authorization",
            basicAuthHeaderValue(OPERATOR_USER_NAME, new SecureString(TEST_PASSWORD.toCharArray()))));
        final GetUsersRequest getUsersRequest = new GetUsersRequest();
        final ElasticsearchSecurityException e = expectThrows(ElasticsearchSecurityException.class,
            () -> client.execute(GetUsersAction.INSTANCE, getUsersRequest).actionGet());
        assertThat(e.getMessage(), containsString("is unauthorized for user"));
    }
}
