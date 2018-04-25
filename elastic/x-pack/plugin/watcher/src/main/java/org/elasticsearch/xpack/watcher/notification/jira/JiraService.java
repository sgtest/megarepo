/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.watcher.notification.jira;

import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Setting.Property;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.xpack.watcher.common.http.HttpClient;
import org.elasticsearch.xpack.watcher.notification.NotificationService;

import java.util.Arrays;
import java.util.List;

/**
 * A component to store Atlassian's JIRA credentials.
 *
 * https://www.atlassian.com/software/jira
 */
public class JiraService extends NotificationService<JiraAccount> {

    private static final Setting<String> SETTING_DEFAULT_ACCOUNT =
            Setting.simpleString("xpack.notification.jira.default_account", Property.Dynamic, Property.NodeScope);

    private static final Setting.AffixSetting<Boolean> SETTING_ALLOW_HTTP =
            Setting.affixKeySetting("xpack.notification.jira.account.", "allow_http",
                    (key) -> Setting.boolSetting(key, false, Property.Dynamic, Property.NodeScope));

    private static final Setting.AffixSetting<String> SETTING_URL =
            Setting.affixKeySetting("xpack.notification.jira.account.", "url",
                    (key) -> Setting.simpleString(key, Property.Dynamic, Property.NodeScope, Property.Filtered));

    private static final Setting.AffixSetting<String> SETTING_USER =
            Setting.affixKeySetting("xpack.notification.jira.account.", "user",
                    (key) -> Setting.simpleString(key, Property.Dynamic, Property.NodeScope, Property.Filtered));

    private static final Setting.AffixSetting<String> SETTING_PASSWORD =
            Setting.affixKeySetting("xpack.notification.jira.account.", "password",
                    (key) -> Setting.simpleString(key, Property.Dynamic, Property.NodeScope, Property.Filtered, Property.Deprecated));

    private static final Setting.AffixSetting<String> SETTING_SECURE_USER =
            Setting.affixKeySetting("xpack.notification.jira.account.", "secure_user",
                    (key) -> Setting.simpleString(key, Property.Dynamic, Property.NodeScope, Property.Filtered));

    private static final Setting.AffixSetting<String> SETTING_SECURE_URL =
            Setting.affixKeySetting("xpack.notification.jira.account.", "secure_url",
                    (key) -> Setting.simpleString(key, Property.Dynamic, Property.NodeScope, Property.Filtered));

    private static final Setting.AffixSetting<String> SETTING_SECURE_PASSWORD =
            Setting.affixKeySetting("xpack.notification.jira.account.", "secure_password",
                    (key) -> Setting.simpleString(key, Property.Dynamic, Property.NodeScope, Property.Filtered));

    private static final Setting.AffixSetting<Settings> SETTING_DEFAULTS =
            Setting.affixKeySetting("xpack.notification.jira.account.", "issue_defaults",
                    (key) -> Setting.groupSetting(key + ".", Property.Dynamic, Property.NodeScope));

    private final HttpClient httpClient;

    public JiraService(Settings settings, HttpClient httpClient, ClusterSettings clusterSettings) {
        super(settings, "jira");
        this.httpClient = httpClient;
        clusterSettings.addSettingsUpdateConsumer(this::setAccountSetting, getSettings());
        // ensure logging of setting changes
        clusterSettings.addSettingsUpdateConsumer(SETTING_DEFAULT_ACCOUNT, (s) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_ALLOW_HTTP, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_URL, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_USER, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_PASSWORD, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SECURE_USER, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SECURE_URL, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SECURE_PASSWORD, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_DEFAULTS, (s, o) -> {}, (s, o) -> {});
        // do an initial load
        setAccountSetting(settings);
    }

    @Override
    protected JiraAccount createAccount(String name, Settings settings) {
        return new JiraAccount(name, settings, httpClient);
    }

    public static List<Setting<?>> getSettings() {
        return Arrays.asList(SETTING_ALLOW_HTTP, SETTING_URL, SETTING_USER, SETTING_PASSWORD, SETTING_SECURE_USER,
                SETTING_SECURE_PASSWORD, SETTING_SECURE_URL, SETTING_DEFAULTS, SETTING_DEFAULT_ACCOUNT);
    }
}
