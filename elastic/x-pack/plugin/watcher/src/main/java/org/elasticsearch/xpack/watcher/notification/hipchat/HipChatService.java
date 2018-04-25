/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.watcher.notification.hipchat;

import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.SecureSetting;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.settings.SettingsException;
import org.elasticsearch.xpack.watcher.common.http.HttpClient;
import org.elasticsearch.xpack.watcher.notification.NotificationService;

import java.util.Arrays;
import java.util.List;

/**
 * A component to store hipchat credentials.
 */
public class HipChatService extends NotificationService<HipChatAccount> {

    private static final Setting<String> SETTING_DEFAULT_ACCOUNT =
            Setting.simpleString("xpack.notification.hipchat.default_account", Setting.Property.Dynamic, Setting.Property.NodeScope);

    static final Setting<String> SETTING_DEFAULT_HOST =
            Setting.simpleString("xpack.notification.hipchat.host", Setting.Property.Dynamic, Setting.Property.NodeScope);

    static final Setting<Integer> SETTING_DEFAULT_PORT =
            Setting.intSetting("xpack.notification.hipchat.port", 443, Setting.Property.Dynamic, Setting.Property.NodeScope);

    private static final Setting.AffixSetting<String> SETTING_AUTH_TOKEN =
            Setting.affixKeySetting("xpack.notification.hipchat.account.", "auth_token",
                    (key) -> Setting.simpleString(key, Setting.Property.Dynamic, Setting.Property.NodeScope, Setting.Property.Filtered,
                            Setting.Property.Deprecated));

    private static final Setting.AffixSetting<String> SETTING_AUTH_TOKEN_SECURE =
            Setting.affixKeySetting("xpack.notification.hipchat.account.", "secure_auth_token",
                    (key) -> SecureSetting.simpleString(key, Setting.Property.Dynamic, Setting.Property.NodeScope,
                            Setting.Property.Filtered));

    private static final Setting.AffixSetting<String> SETTING_PROFILE =
            Setting.affixKeySetting("xpack.notification.hipchat.account.", "profile",
                    (key) -> Setting.simpleString(key, Setting.Property.Dynamic, Setting.Property.NodeScope));

    private static final Setting.AffixSetting<String> SETTING_ROOM =
            Setting.affixKeySetting("xpack.notification.hipchat.account.", "room",
                    (key) -> Setting.simpleString(key, Setting.Property.Dynamic, Setting.Property.NodeScope));

    private static final Setting.AffixSetting<String> SETTING_HOST =
            Setting.affixKeySetting("xpack.notification.hipchat.account.", "host",
                    (key) -> Setting.simpleString(key, Setting.Property.Dynamic, Setting.Property.NodeScope));

    private static final Setting.AffixSetting<Integer> SETTING_PORT =
            Setting.affixKeySetting("xpack.notification.hipchat.account.", "port",
                    (key) -> Setting.intSetting(key, 443, Setting.Property.Dynamic, Setting.Property.NodeScope));

    private static final Setting.AffixSetting<Settings> SETTING_MESSAGE_DEFAULTS =
            Setting.affixKeySetting("xpack.notification.hipchat.account.", "message",
                    (key) -> Setting.groupSetting(key + ".", Setting.Property.Dynamic, Setting.Property.NodeScope));


    private final HttpClient httpClient;
    private HipChatServer defaultServer;

    public HipChatService(Settings settings, HttpClient httpClient, ClusterSettings clusterSettings) {
        super(settings, "hipchat");
        this.httpClient = httpClient;
        clusterSettings.addSettingsUpdateConsumer(this::setAccountSetting, getSettings());
        // ensure logging of setting changes
        clusterSettings.addSettingsUpdateConsumer(SETTING_DEFAULT_ACCOUNT, (s) -> {});
        clusterSettings.addSettingsUpdateConsumer(SETTING_DEFAULT_HOST, (s) -> {});
        clusterSettings.addSettingsUpdateConsumer(SETTING_DEFAULT_PORT, (s) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_AUTH_TOKEN, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_AUTH_TOKEN_SECURE, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_PROFILE, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_ROOM, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_HOST, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_PORT, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_MESSAGE_DEFAULTS, (s, o) -> {}, (s, o) -> {});

        setAccountSetting(settings);
    }

    @Override
    protected synchronized void setAccountSetting(Settings settings) {
        defaultServer = new HipChatServer(settings.getByPrefix("xpack.notification.hipchat."));
        super.setAccountSetting(settings);
    }

    @Override
    protected HipChatAccount createAccount(String name, Settings accountSettings) {
        HipChatAccount.Profile profile = HipChatAccount.Profile.resolve(accountSettings, "profile", null);
        if (profile == null) {
            throw new SettingsException("missing [profile] setting for hipchat account [" + name + "]");
        }
        return profile.createAccount(name, accountSettings, defaultServer, httpClient, logger);
    }

    public static List<Setting<?>> getSettings() {
        return Arrays.asList(SETTING_DEFAULT_ACCOUNT, SETTING_AUTH_TOKEN, SETTING_AUTH_TOKEN_SECURE, SETTING_PROFILE, SETTING_ROOM,
                SETTING_MESSAGE_DEFAULTS, SETTING_DEFAULT_HOST, SETTING_DEFAULT_PORT, SETTING_HOST, SETTING_PORT);
    }
}
