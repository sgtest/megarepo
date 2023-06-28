package com.sourcegraph.config;

import com.intellij.openapi.options.Configurable;
import com.intellij.openapi.project.Project;
import com.intellij.util.messages.MessageBus;
import javax.swing.*;
import org.jetbrains.annotations.Nls;
import org.jetbrains.annotations.Nullable;

/** Provides controller functionality for application settings. */
public class SettingsConfigurable implements Configurable {
  private final Project project;
  private SettingsComponent mySettingsComponent;

  public SettingsConfigurable(Project project) {
    this.project = project;
  }

  @Nls(capitalization = Nls.Capitalization.Title)
  @Override
  public String getDisplayName() {
    return "Cody AI by Sourcegraph";
  }

  @Override
  public JComponent getPreferredFocusedComponent() {
    return mySettingsComponent.getPreferredFocusedComponent();
  }

  @Nullable
  @Override
  public JComponent createComponent() {
    mySettingsComponent = new SettingsComponent(project);
    return mySettingsComponent.getPanel();
  }

  @Override
  public boolean isModified() {
    return !mySettingsComponent.getInstanceType().equals(ConfigUtil.getInstanceType(project))
        || !mySettingsComponent.getEnterpriseUrl().equals(ConfigUtil.getEnterpriseUrl(project))
        || !(mySettingsComponent
                .getDotComAccessToken()
                .equals(ConfigUtil.getDotComAccessToken(project))
            || mySettingsComponent.getDotComAccessToken().isEmpty()
                && ConfigUtil.getDotComAccessToken(project) == null)
        || !(mySettingsComponent
                .getEnterpriseAccessToken()
                .equals(ConfigUtil.getEnterpriseAccessToken(project))
            || mySettingsComponent.getEnterpriseAccessToken().isEmpty()
                && ConfigUtil.getEnterpriseAccessToken(project) == null)
        || !mySettingsComponent
            .getCustomRequestHeaders()
            .equals(ConfigUtil.getCustomRequestHeaders(project))
        || !mySettingsComponent
            .getDefaultBranchName()
            .equals(ConfigUtil.getDefaultBranchName(project))
        || !mySettingsComponent
            .getRemoteUrlReplacements()
            .equals(ConfigUtil.getRemoteUrlReplacements(project))
        || mySettingsComponent.isUrlNotificationDismissed()
            != ConfigUtil.isUrlNotificationDismissed()
        || mySettingsComponent.areCodyCompletionsEnabled()
            != ConfigUtil.areCodyCompletionsEnabled();
  }

  @Override
  public void apply() {
    MessageBus bus = project.getMessageBus();
    PluginSettingChangeActionNotifier publisher =
        bus.syncPublisher(PluginSettingChangeActionNotifier.TOPIC);

    CodyApplicationService aSettings = CodyApplicationService.getInstance();
    CodyProjectService pSettings = CodyService.getInstance(project);

    boolean oldCodyCompletionsEnabled = ConfigUtil.areCodyCompletionsEnabled();
    String oldUrl = ConfigUtil.getSourcegraphUrl(project);
    String oldDotComAccessToken = ConfigUtil.getDotComAccessToken(project);
    String oldEnterpriseAccessToken = ConfigUtil.getEnterpriseAccessToken(project);
    String newUrl = mySettingsComponent.getEnterpriseUrl();
    String newDotComAccessToken = mySettingsComponent.getDotComAccessToken();
    String newEnterpriseAccessToken = mySettingsComponent.getEnterpriseAccessToken();
    String newCustomRequestHeaders = mySettingsComponent.getCustomRequestHeaders();
    boolean newCodyCompletionsEnabled = mySettingsComponent.areCodyCompletionsEnabled();
    PluginSettingChangeContext context =
        new PluginSettingChangeContext(
            oldUrl,
            oldDotComAccessToken,
            oldEnterpriseAccessToken,
            oldCodyCompletionsEnabled,
            newUrl,
            newDotComAccessToken,
            newEnterpriseAccessToken,
            newCustomRequestHeaders,
            newCodyCompletionsEnabled);

    publisher.beforeAction(context);

    if (pSettings.instanceType != null) {
      pSettings.instanceType = mySettingsComponent.getInstanceType().name();
    } else {
      aSettings.instanceType = mySettingsComponent.getInstanceType().name();
    }
    if (pSettings.url != null) {
      pSettings.url = newUrl;
    } else {
      aSettings.url = newUrl;
    }
    if (pSettings.dotComAccessToken != null) {
      pSettings.dotComAccessToken = newDotComAccessToken;
    } else {
      aSettings.dotComAccessToken = newDotComAccessToken;
    }
    if (pSettings.enterpriseAccessToken != null) {
      pSettings.enterpriseAccessToken = newEnterpriseAccessToken;
    } else {
      aSettings.enterpriseAccessToken = newEnterpriseAccessToken;
    }
    if (pSettings.customRequestHeaders != null) {
      pSettings.customRequestHeaders = mySettingsComponent.getCustomRequestHeaders();
    } else {
      aSettings.customRequestHeaders = mySettingsComponent.getCustomRequestHeaders();
    }
    if (pSettings.defaultBranch != null) {
      pSettings.defaultBranch = mySettingsComponent.getDefaultBranchName();
    } else {
      aSettings.defaultBranch = mySettingsComponent.getDefaultBranchName();
    }
    if (pSettings.remoteUrlReplacements != null) {
      pSettings.remoteUrlReplacements = mySettingsComponent.getRemoteUrlReplacements();
    } else {
      aSettings.remoteUrlReplacements = mySettingsComponent.getRemoteUrlReplacements();
    }
    aSettings.isUrlNotificationDismissed = mySettingsComponent.isUrlNotificationDismissed();
    aSettings.areCodyCompletionsEnabled = newCodyCompletionsEnabled;

    publisher.afterAction(context);
  }

  @Override
  public void reset() {
    mySettingsComponent.setInstanceType(ConfigUtil.getInstanceType(project));
    mySettingsComponent.setEnterpriseUrl(ConfigUtil.getEnterpriseUrl(project));
    String dotComAccessToken = ConfigUtil.getDotComAccessToken(project);
    mySettingsComponent.setDotComAccessToken(dotComAccessToken != null ? dotComAccessToken : "");
    String enterpriseAccessToken = ConfigUtil.getEnterpriseAccessToken(project);
    mySettingsComponent.setEnterpriseAccessToken(
        enterpriseAccessToken != null ? enterpriseAccessToken : "");
    mySettingsComponent.setCustomRequestHeaders(ConfigUtil.getCustomRequestHeaders(project));
    String defaultBranchName = ConfigUtil.getDefaultBranchName(project);
    mySettingsComponent.setDefaultBranchName(defaultBranchName);
    String remoteUrlReplacements = ConfigUtil.getRemoteUrlReplacements(project);
    mySettingsComponent.setRemoteUrlReplacements(remoteUrlReplacements);
    mySettingsComponent.setUrlNotificationDismissedEnabled(ConfigUtil.isUrlNotificationDismissed());
    mySettingsComponent.setCodyCompletionsEnabled(ConfigUtil.areCodyCompletionsEnabled());
  }

  @Override
  public void disposeUIResources() {
    mySettingsComponent = null;
  }
}
