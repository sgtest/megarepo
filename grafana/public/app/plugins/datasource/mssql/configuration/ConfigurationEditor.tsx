import { css } from '@emotion/css';
import React, { SyntheticEvent } from 'react';

import {
  DataSourcePluginOptionsEditorProps,
  GrafanaTheme2,
  onUpdateDatasourceJsonDataOption,
  onUpdateDatasourceSecureJsonDataOption,
  SelectableValue,
  updateDatasourcePluginJsonDataOption,
  updateDatasourcePluginResetOption,
} from '@grafana/data';
import { ConfigSection, ConfigSubSection, DataSourceDescription } from '@grafana/experimental';
import {
  Alert,
  FieldSet,
  Input,
  Link,
  SecretInput,
  Select,
  useStyles2,
  SecureSocksProxySettings,
  Divider,
  Field,
  Switch,
} from '@grafana/ui';
import { NumberInput } from 'app/core/components/OptionsUI/NumberInput';
import { config } from 'app/core/config';
import { ConnectionLimits } from 'app/features/plugins/sql/components/configuration/ConnectionLimits';
import { useMigrateDatabaseFields } from 'app/features/plugins/sql/components/configuration/useMigrateDatabaseFields';

import { AzureAuthSettings } from '../azureauth/AzureAuthSettings';
import {
  MSSQLAuthenticationType,
  MSSQLEncryptOptions,
  MssqlOptions,
  AzureAuthConfigType,
  MssqlSecureOptions,
} from '../types';

const LONG_WIDTH = 40;

export const ConfigurationEditor = (props: DataSourcePluginOptionsEditorProps<MssqlOptions, MssqlSecureOptions>) => {
  useMigrateDatabaseFields(props);

  const { options: dsSettings, onOptionsChange } = props;
  const styles = useStyles2(getStyles);
  const jsonData = dsSettings.jsonData;
  const azureAuthIsSupported = config.azureAuthEnabled;

  const azureAuthSettings: AzureAuthConfigType = {
    azureAuthIsSupported,
    azureAuthSettingsUI: AzureAuthSettings,
  };

  const onResetPassword = () => {
    updateDatasourcePluginResetOption(props, 'password');
  };

  const onDSOptionChanged = (property: keyof MssqlOptions) => {
    return (event: SyntheticEvent<HTMLInputElement>) => {
      onOptionsChange({ ...dsSettings, ...{ [property]: event.currentTarget.value } });
    };
  };

  const onSkipTLSVerifyChanged = (event: SyntheticEvent<HTMLInputElement>) => {
    updateDatasourcePluginJsonDataOption(props, 'tlsSkipVerify', event.currentTarget.checked);
  };

  const onEncryptChanged = (value: SelectableValue) => {
    updateDatasourcePluginJsonDataOption(props, 'encrypt', value.value);
  };

  const onAuthenticationMethodChanged = (value: SelectableValue) => {
    onOptionsChange({
      ...dsSettings,
      ...{
        jsonData: { ...jsonData, ...{ authenticationType: value.value }, azureCredentials: undefined },
        secureJsonData: { ...dsSettings.secureJsonData, ...{ password: '' } },
        secureJsonFields: { ...dsSettings.secureJsonFields, ...{ password: false } },
        user: '',
      },
    });
  };

  const onConnectionTimeoutChanged = (connectionTimeout?: number) => {
    updateDatasourcePluginJsonDataOption(props, 'connectionTimeout', connectionTimeout ?? 0);
  };

  const buildAuthenticationOptions = (): Array<SelectableValue<MSSQLAuthenticationType>> => {
    const basicAuthenticationOptions: Array<SelectableValue<MSSQLAuthenticationType>> = [
      { value: MSSQLAuthenticationType.sqlAuth, label: 'SQL Server Authentication' },
      { value: MSSQLAuthenticationType.windowsAuth, label: 'Windows Authentication' },
    ];

    if (azureAuthIsSupported) {
      return [
        ...basicAuthenticationOptions,
        { value: MSSQLAuthenticationType.azureAuth, label: 'Azure AD Authentication' },
      ];
    }

    return basicAuthenticationOptions;
  };

  const encryptOptions: Array<SelectableValue<string>> = [
    { value: MSSQLEncryptOptions.disable, label: 'disable' },
    { value: MSSQLEncryptOptions.false, label: 'false' },
    { value: MSSQLEncryptOptions.true, label: 'true' },
  ];

  return (
    <>
      <DataSourceDescription
        dataSourceName="Microsoft SQL Server"
        docsLink="https://grafana.com/docs/grafana/latest/datasources/mssql/"
        hasRequiredFields
      />
      <Alert title="User Permission" severity="info">
        The database user should only be granted SELECT permissions on the specified database and tables you want to
        query. Grafana does not validate that queries are safe so queries can contain any SQL statement. For example,
        statements like <code>USE otherdb;</code> and <code>DROP TABLE user;</code> would be executed. To protect
        against this we <em>highly</em> recommend you create a specific MS SQL user with restricted permissions. Check
        out the{' '}
        <Link rel="noreferrer" target="_blank" href="http://docs.grafana.org/features/datasources/mssql/">
          Microsoft SQL Server Data Source Docs
        </Link>{' '}
        for more information.
      </Alert>
      <Divider />
      <ConfigSection title="Connection">
        <Field label="Host" required invalid={!dsSettings.url} error={'Host is required'}>
          <Input
            width={LONG_WIDTH}
            name="host"
            type="text"
            value={dsSettings.url || ''}
            placeholder="localhost:1433"
            onChange={onDSOptionChanged('url')}
          />
        </Field>
        <Field label="Database" required invalid={!jsonData.database} error={'Database is required'}>
          <Input
            width={LONG_WIDTH}
            name="database"
            value={jsonData.database || ''}
            placeholder="database name"
            onChange={onUpdateDatasourceJsonDataOption(props, 'database')}
          />
        </Field>
      </ConfigSection>

      <ConfigSection title="TLS/SSL Auth">
        <Field
          htmlFor="encrypt"
          description={
            <>
              Determines whether or to which extent a secure SSL TCP/IP connection will be negotiated with the server.
              <ul className={styles.ulPadding}>
                <li>
                  <i>disable</i> - Data sent between client and server is not encrypted.
                </li>
                <li>
                  <i>false</i> - Data sent between client and server is not encrypted beyond the login packet. (default)
                </li>
                <li>
                  <i>true</i> - Data sent between client and server is encrypted.
                </li>
              </ul>
              If you&apos;re using an older version of Microsoft SQL Server like 2008 and 2008R2 you may need to disable
              encryption to be able to connect.
            </>
          }
          label="Encrypt"
        >
          <Select
            options={encryptOptions}
            value={jsonData.encrypt || MSSQLEncryptOptions.false}
            inputId="encrypt"
            onChange={onEncryptChanged}
            width={LONG_WIDTH}
          />
        </Field>

        {jsonData.encrypt === MSSQLEncryptOptions.true ? (
          <>
            <Field htmlFor="skipTlsVerify" label="Skip TLS Verify">
              <Switch id="skipTlsVerify" onChange={onSkipTLSVerifyChanged} value={jsonData.tlsSkipVerify || false} />
            </Field>
            {jsonData.tlsSkipVerify ? null : (
              <>
                <Field
                  description={
                    <span>
                      Path to file containing the public key certificate of the CA that signed the SQL Server
                      certificate. Needed when the server certificate is self signed.
                    </span>
                  }
                  label="TLS/SSL Root Certificate"
                >
                  <Input
                    value={jsonData.sslRootCertFile || ''}
                    onChange={onUpdateDatasourceJsonDataOption(props, 'sslRootCertFile')}
                    placeholder="TLS/SSL root certificate file path"
                    width={LONG_WIDTH}
                  />
                </Field>
                <Field label="Hostname in server certificate">
                  <Input
                    placeholder="Common Name (CN) in server certificate"
                    value={jsonData.serverName || ''}
                    onChange={onUpdateDatasourceJsonDataOption(props, 'serverName')}
                    width={LONG_WIDTH}
                  />
                </Field>
              </>
            )}
          </>
        ) : null}
      </ConfigSection>

      <ConfigSection title="Authentication">
        <Field
          label="Authentication Type"
          htmlFor="authenticationType"
          description={
            <ul className={styles.ulPadding}>
              <li>
                <i>SQL Server Authentication</i> This is the default mechanism to connect to MS SQL Server. Enter the
                SQL Server Authentication login or the Windows Authentication login in the DOMAIN\User format.
              </li>
              <li>
                <i>Windows Authentication</i> Windows Integrated Security - single sign on for users who are already
                logged onto Windows and have enabled this option for MS SQL Server.
              </li>
              {azureAuthIsSupported && (
                <li>
                  <i>Azure Authentication</i> Securely authenticate and access Azure resources and applications using
                  Azure AD credentials - Managed Service Identity and Client Secret Credentials are supported.
                </li>
              )}
            </ul>
          }
        >
          <Select
            // Default to basic authentication of none is set
            value={jsonData.authenticationType || MSSQLAuthenticationType.sqlAuth}
            inputId="authenticationType"
            options={buildAuthenticationOptions()}
            onChange={onAuthenticationMethodChanged}
            width={LONG_WIDTH}
          />
        </Field>

        {/* Basic SQL auth. Render if authType === MSSQLAuthenticationType.sqlAuth OR
        if no authType exists, which will be the case when creating a new data source */}
        {(jsonData.authenticationType === MSSQLAuthenticationType.sqlAuth || !jsonData.authenticationType) && (
          <>
            <Field label="Username" required invalid={!dsSettings.user} error={'Username is required'}>
              <Input
                value={dsSettings.user || ''}
                placeholder="user"
                onChange={onDSOptionChanged('user')}
                width={LONG_WIDTH}
              />
            </Field>
            <Field
              label="Password"
              required
              invalid={!dsSettings.secureJsonFields.password && !dsSettings.secureJsonData?.password}
              error={'Password is required'}
            >
              <SecretInput
                width={LONG_WIDTH}
                placeholder="Password"
                isConfigured={dsSettings.secureJsonFields && dsSettings.secureJsonFields.password}
                onReset={onResetPassword}
                onChange={onUpdateDatasourceSecureJsonDataOption(props, 'password')}
                required
              />
            </Field>
          </>
        )}

        {azureAuthIsSupported && jsonData.authenticationType === MSSQLAuthenticationType.azureAuth && (
          <FieldSet label="Azure Authentication Settings">
            <azureAuthSettings.azureAuthSettingsUI dataSourceConfig={dsSettings} onChange={onOptionsChange} />
          </FieldSet>
        )}
      </ConfigSection>

      <Divider />
      <ConfigSection
        title="Additional settings"
        description="Additional settings are optional settings that can be configured for more control over your data source. This includes connection limits, connection timeout, group-by time interval, and Secure Socks Proxy."
        isCollapsible={true}
        isInitiallyOpen={true}
      >
        <ConnectionLimits options={dsSettings} onOptionsChange={onOptionsChange} />

        <ConfigSubSection title="Connection details">
          <Field
            description={
              <span>
                A lower limit for the auto group by time interval. Recommended to be set to write frequency, for example
                <code>1m</code> if your data is written every minute.
              </span>
            }
            label="Min time interval"
          >
            <Input
              width={LONG_WIDTH}
              placeholder="1m"
              value={jsonData.timeInterval || ''}
              onChange={onUpdateDatasourceJsonDataOption(props, 'timeInterval')}
            />
          </Field>
          <Field
            description={
              <span>
                The number of seconds to wait before canceling the request when connecting to the database. The default
                is <code>0</code>, meaning no timeout.
              </span>
            }
            label="Connection timeout"
          >
            <NumberInput
              width={LONG_WIDTH}
              placeholder="60"
              min={0}
              value={jsonData.connectionTimeout}
              onChange={onConnectionTimeoutChanged}
            />
          </Field>
        </ConfigSubSection>
        {config.secureSocksDSProxyEnabled && (
          <SecureSocksProxySettings options={dsSettings} onOptionsChange={onOptionsChange} />
        )}
      </ConfigSection>
    </>
  );
};

function getStyles(theme: GrafanaTheme2) {
  return {
    ulPadding: css({
      margin: theme.spacing(1, 0),
      paddingLeft: theme.spacing(5),
    }),
  };
}
