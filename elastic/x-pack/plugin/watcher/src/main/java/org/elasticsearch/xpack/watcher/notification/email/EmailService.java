/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.watcher.notification.email;

import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Setting.Property;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.xpack.core.watcher.crypto.CryptoService;
import org.elasticsearch.xpack.watcher.notification.NotificationService;

import javax.mail.MessagingException;
import java.util.Arrays;
import java.util.List;

/**
 * A component to store email credentials and handle sending email notifications.
 */
public class EmailService extends NotificationService<Account> {

    private static final Setting<String> SETTING_DEFAULT_ACCOUNT =
        Setting.simpleString("xpack.notification.email.default_account", Property.Dynamic, Property.NodeScope);

    private static final Setting.AffixSetting<String> SETTING_PROFILE =
            Setting.affixKeySetting("xpack.notification.email.account.", "profile",
                    (key) -> Setting.simpleString(key, Property.Dynamic, Property.NodeScope));

    private static final Setting.AffixSetting<Settings> SETTING_EMAIL_DEFAULTS =
            Setting.affixKeySetting("xpack.notification.email.account.", "email_defaults",
                    (key) -> Setting.groupSetting(key + ".", Property.Dynamic, Property.NodeScope));

    // settings that can be configured as smtp properties
    private static final Setting.AffixSetting<Boolean> SETTING_SMTP_AUTH =
            Setting.affixKeySetting("xpack.notification.email.account.", "smtp.auth",
                    (key) -> Setting.boolSetting(key, false, Property.Dynamic, Property.NodeScope));

    private static final Setting.AffixSetting<Boolean> SETTING_SMTP_STARTTLS_ENABLE =
            Setting.affixKeySetting("xpack.notification.email.account.", "smtp.starttls.enable",
                    (key) -> Setting.boolSetting(key, false, Property.Dynamic, Property.NodeScope));

    private static final Setting.AffixSetting<Boolean> SETTING_SMTP_STARTTLS_REQUIRED =
            Setting.affixKeySetting("xpack.notification.email.account.", "smtp.starttls.required",
                    (key) -> Setting.boolSetting(key, false, Property.Dynamic, Property.NodeScope));

    private static final Setting.AffixSetting<String> SETTING_SMTP_HOST =
            Setting.affixKeySetting("xpack.notification.email.account.", "smtp.host",
                    (key) -> Setting.simpleString(key, Property.Dynamic, Property.NodeScope));

    private static final Setting.AffixSetting<Integer> SETTING_SMTP_PORT =
            Setting.affixKeySetting("xpack.notification.email.account.", "smtp.port",
                    (key) -> Setting.intSetting(key, 587, Property.Dynamic, Property.NodeScope));

    private static final Setting.AffixSetting<String> SETTING_SMTP_USER =
            Setting.affixKeySetting("xpack.notification.email.account.", "smtp.user",
                    (key) -> Setting.simpleString(key, Property.Dynamic, Property.NodeScope));

    private static final Setting.AffixSetting<String> SETTING_SMTP_PASSWORD =
            Setting.affixKeySetting("xpack.notification.email.account.", "smtp.password",
                    (key) -> Setting.simpleString(key, Property.Dynamic, Property.NodeScope, Property.Filtered));

    private static final Setting.AffixSetting<TimeValue> SETTING_SMTP_TIMEOUT =
            Setting.affixKeySetting("xpack.notification.email.account.", "smtp.timeout",
                    (key) -> Setting.timeSetting(key, TimeValue.timeValueMinutes(2), Property.Dynamic, Property.NodeScope));

    private static final Setting.AffixSetting<TimeValue> SETTING_SMTP_CONNECTION_TIMEOUT =
            Setting.affixKeySetting("xpack.notification.email.account.", "smtp.connection_timeout",
                    (key) -> Setting.timeSetting(key, TimeValue.timeValueMinutes(2), Property.Dynamic, Property.NodeScope));

    private static final Setting.AffixSetting<TimeValue> SETTING_SMTP_WRITE_TIMEOUT =
            Setting.affixKeySetting("xpack.notification.email.account.", "smtp.write_timeout",
                    (key) -> Setting.timeSetting(key, TimeValue.timeValueMinutes(2), Property.Dynamic, Property.NodeScope));

    private static final Setting.AffixSetting<String> SETTING_SMTP_LOCAL_ADDRESS =
            Setting.affixKeySetting("xpack.notification.email.account.", "smtp.local_address",
                    (key) -> Setting.simpleString(key, Property.Dynamic, Property.NodeScope));

    private static final Setting.AffixSetting<Integer> SETTING_SMTP_LOCAL_PORT =
            Setting.affixKeySetting("xpack.notification.email.account.", "smtp.local_port",
                    (key) -> Setting.intSetting(key, 25, Property.Dynamic, Property.NodeScope));

    private static final Setting.AffixSetting<Boolean> SETTING_SMTP_SEND_PARTIAL =
            Setting.affixKeySetting("xpack.notification.email.account.", "smtp.send_partial",
                    (key) -> Setting.boolSetting(key, false, Property.Dynamic, Property.NodeScope));

    private static final Setting.AffixSetting<Boolean> SETTING_SMTP_WAIT_ON_QUIT =
            Setting.affixKeySetting("xpack.notification.email.account.", "smtp.wait_on_quit",
                    (key) -> Setting.boolSetting(key, true, Property.Dynamic, Property.NodeScope));

    private final CryptoService cryptoService;

    public EmailService(Settings settings, @Nullable CryptoService cryptoService, ClusterSettings clusterSettings) {
        super(settings, "email");
        this.cryptoService = cryptoService;
        clusterSettings.addSettingsUpdateConsumer(this::setAccountSetting, getSettings());
        // ensure logging of setting changes
        clusterSettings.addSettingsUpdateConsumer(SETTING_DEFAULT_ACCOUNT, (s) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_PROFILE, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_EMAIL_DEFAULTS, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SMTP_AUTH, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SMTP_STARTTLS_ENABLE, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SMTP_STARTTLS_REQUIRED, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SMTP_HOST, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SMTP_PORT, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SMTP_USER, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SMTP_PASSWORD, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SMTP_TIMEOUT, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SMTP_CONNECTION_TIMEOUT, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SMTP_WRITE_TIMEOUT, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SMTP_LOCAL_ADDRESS, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SMTP_LOCAL_PORT, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SMTP_SEND_PARTIAL, (s, o) -> {}, (s, o) -> {});
        clusterSettings.addAffixUpdateConsumer(SETTING_SMTP_WAIT_ON_QUIT, (s, o) -> {}, (s, o) -> {});
        // do an initial load
        setAccountSetting(settings);
    }

    @Override
    protected Account createAccount(String name, Settings accountSettings) {
        Account.Config config = new Account.Config(name, accountSettings);
        return new Account(config, cryptoService, logger);
    }

    public EmailSent send(Email email, Authentication auth, Profile profile, String accountName) throws MessagingException {
        Account account = getAccount(accountName);
        if (account == null) {
            throw new IllegalArgumentException("failed to send email with subject [" + email.subject() + "] via account [" + accountName
                + "]. account does not exist");
        }
        return send(email, auth, profile, account);
    }

    private EmailSent send(Email email, Authentication auth, Profile profile, Account account) throws MessagingException {
        assert account != null;
        try {
            email = account.send(email, auth, profile);
        } catch (MessagingException me) {
            throw new MessagingException("failed to send email with subject [" + email.subject() + "] via account [" + account.name() +
                "]", me);
        }
        return new EmailSent(account.name(), email);
    }

    public static class EmailSent {

        private final String account;
        private final Email email;

        public EmailSent(String account, Email email) {
            this.account = account;
            this.email = email;
        }

        public String account() {
            return account;
        }

        public Email email() {
            return email;
        }
    }

    public static List<Setting<?>> getSettings() {
        return Arrays.asList(SETTING_DEFAULT_ACCOUNT, SETTING_PROFILE, SETTING_EMAIL_DEFAULTS, SETTING_SMTP_AUTH, SETTING_SMTP_HOST,
                SETTING_SMTP_PASSWORD, SETTING_SMTP_PORT, SETTING_SMTP_STARTTLS_ENABLE, SETTING_SMTP_USER, SETTING_SMTP_STARTTLS_REQUIRED,
                SETTING_SMTP_TIMEOUT, SETTING_SMTP_CONNECTION_TIMEOUT, SETTING_SMTP_WRITE_TIMEOUT, SETTING_SMTP_LOCAL_ADDRESS,
                SETTING_SMTP_LOCAL_PORT, SETTING_SMTP_SEND_PARTIAL, SETTING_SMTP_WAIT_ON_QUIT);
    }

}
