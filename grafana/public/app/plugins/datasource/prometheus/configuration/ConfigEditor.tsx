import { css } from '@emotion/css';
import React, { useRef } from 'react';

import { SIGV4ConnectionConfig } from '@grafana/aws-sdk';
import { DataSourcePluginOptionsEditorProps, DataSourceSettings, GrafanaTheme2 } from '@grafana/data';
import { ConfigSection, DataSourceDescription } from '@grafana/experimental';
import { Alert, DataSourceHttpSettings, FieldValidationMessage, useTheme2 } from '@grafana/ui';
import { config } from 'app/core/config';

import { PromOptions } from '../types';

import { AlertingSettingsOverhaul } from './AlertingSettingsOverhaul';
import { AzureAuthSettings } from './AzureAuthSettings';
import { hasCredentials, setDefaultCredentials, resetCredentials } from './AzureCredentialsConfig';
import { DataSourcehttpSettingsOverhaul } from './DataSourceHttpSettingsOverhaul';
import { PromSettings } from './PromSettings';
import { AdvancedHttpSettings } from './overhaul/AdvancedHttpSettings';

export const PROM_CONFIG_LABEL_WIDTH = 30;

export type Props = DataSourcePluginOptionsEditorProps<PromOptions>;

export const ConfigEditor = (props: Props) => {
  const { options, onOptionsChange } = props;

  const prometheusConfigOverhaulAuth = config.featureToggles.prometheusConfigOverhaulAuth;

  // use ref so this is evaluated only first time it renders and the select does not disappear suddenly.
  const showAccessOptions = useRef(props.options.access === 'direct');

  const azureAuthSettings = {
    azureAuthSupported: config.azureAuthEnabled,
    getAzureAuthEnabled: (config: DataSourceSettings<any, any>): boolean => hasCredentials(config),
    setAzureAuthEnabled: (config: DataSourceSettings<any, any>, enabled: boolean) =>
      enabled ? setDefaultCredentials(config) : resetCredentials(config),
    azureSettingsUI: AzureAuthSettings,
  };

  const theme = useTheme2();
  const styles = overhaulStyles(theme);

  return (
    <>
      {options.access === 'direct' && (
        <Alert title="Error" severity="error">
          Browser access mode in the Prometheus data source is no longer available. Switch to server access mode.
        </Alert>
      )}
      {/* WRAP IN FEATURE TOGGLE */}
      {prometheusConfigOverhaulAuth ? (
        <>
          <DataSourceDescription
            dataSourceName="Prometheus"
            docsLink="https://grafana.com/docs/grafana/latest/datasources/prometheus/configure-prometheus-data-source/"
          />
          <hr className={`${styles.hrTopSpace} ${styles.hrBottomSpace}`} />
          <DataSourcehttpSettingsOverhaul
            options={options}
            onOptionsChange={onOptionsChange}
            azureAuthSettings={azureAuthSettings}
            sigV4AuthToggleEnabled={config.sigV4AuthEnabled}
            renderSigV4Editor={
              <SIGV4ConnectionConfig inExperimentalAuthComponent={true} {...props}></SIGV4ConnectionConfig>
            }
            secureSocksDSProxyEnabled={config.secureSocksDSProxyEnabled}
          />
        </>
      ) : (
        <DataSourceHttpSettings
          defaultUrl="http://localhost:9090"
          dataSourceConfig={options}
          showAccessOptions={showAccessOptions.current}
          onChange={onOptionsChange}
          sigV4AuthToggleEnabled={config.sigV4AuthEnabled}
          azureAuthSettings={azureAuthSettings}
          renderSigV4Editor={<SIGV4ConnectionConfig {...props}></SIGV4ConnectionConfig>}
          secureSocksDSProxyEnabled={config.secureSocksDSProxyEnabled}
          urlLabel="Prometheus server URL"
          urlDocs={docsTip()}
        />
      )}
      {prometheusConfigOverhaulAuth ? (
        <>
          <hr />
          <ConfigSection
            className={styles.advancedSettings}
            title="Advanced settings"
            description="Additional settings are optional settings that can be configured for more control over your data source."
          >
            <AdvancedHttpSettings
              className={styles.advancedHTTPSettingsMargin}
              config={options}
              onChange={onOptionsChange}
            />
            <AlertingSettingsOverhaul<PromOptions> options={options} onOptionsChange={onOptionsChange} />
            <PromSettings options={options} onOptionsChange={onOptionsChange} />
          </ConfigSection>
        </>
      ) : (
        <>
          <hr className={styles.hrTopSpace} />
          <h3 className={styles.sectionHeaderPadding}>Additional settings</h3>
          <p className={`${styles.secondaryGrey} ${styles.subsectionText}`}>
            Additional settings are optional settings that can be configured for more control over your data source.
          </p>

          <AlertingSettingsOverhaul<PromOptions> options={options} onOptionsChange={onOptionsChange} />

          <PromSettings options={options} onOptionsChange={onOptionsChange} />
        </>
      )}
    </>
  );
};
/**
 * Use this to return a url in a tooltip in a field. Don't forget to make the field interactive to be able to click on the tooltip
 * @param url
 * @returns
 */
export function docsTip(url?: string) {
  const docsUrl = 'https://grafana.com/docs/grafana/latest/datasources/prometheus/#configure-the-data-source';

  return (
    <a href={url ? url : docsUrl} target="_blank" rel="noopener noreferrer">
      Visit docs for more details here.
    </a>
  );
}

export const validateInput = (
  input: string,
  pattern: string | RegExp,
  errorMessage?: string
): boolean | JSX.Element => {
  const defaultErrorMessage = 'Value is not valid';
  if (input && !input.match(pattern)) {
    return <FieldValidationMessage>{errorMessage ? errorMessage : defaultErrorMessage}</FieldValidationMessage>;
  } else {
    return true;
  }
};

export function overhaulStyles(theme: GrafanaTheme2) {
  return {
    additionalSettings: css`
      margin-bottom: 25px;
    `,
    secondaryGrey: css`
      color: ${theme.colors.secondary.text};
      opacity: 65%;
    `,
    inlineError: css`
      margin: 0px 0px 4px 245px;
    `,
    switchField: css`
      align-items: center;
    `,
    sectionHeaderPadding: css`
      padding-top: 32px;
    `,
    sectionBottomPadding: css`
      padding-bottom: 28px;
    `,
    subsectionText: css`
      font-size: 12px;
    `,
    hrBottomSpace: css`
      margin-bottom: 56px;
    `,
    hrTopSpace: css`
      margin-top: 50px;
    `,
    textUnderline: css`
      text-decoration: underline;
    `,
    versionMargin: css`
      margin-bottom: 12px;
    `,
    advancedHTTPSettingsMargin: css`
      margin: 24px 0 8px 0;
    `,
    advancedSettings: css`
      padding-top: 32px;
    `,
    alertingTop: css`
      margin-top: 40px !important;
    `,
    overhaulPageHeading: css`
      font-weight: 400;
    `,
    container: css`
      maxwidth: 578;
    `,
  };
}
