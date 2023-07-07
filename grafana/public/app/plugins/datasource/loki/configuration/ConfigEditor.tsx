import React, { useCallback } from 'react';

import { DataSourcePluginOptionsEditorProps, DataSourceSettings } from '@grafana/data';
import { ConfigSection, DataSourceDescription } from '@grafana/experimental';
import { config, reportInteraction } from '@grafana/runtime';
import { DataSourceHttpSettings } from '@grafana/ui';
import { Divider } from 'app/core/components/Divider';

import { LokiOptions } from '../types';

import { AlertingSettings } from './AlertingSettings';
import { DerivedFields } from './DerivedFields';
import { QuerySettings } from './QuerySettings';

export type Props = DataSourcePluginOptionsEditorProps<LokiOptions>;

const makeJsonUpdater =
  <T extends any>(field: keyof LokiOptions) =>
  (options: DataSourceSettings<LokiOptions>, value: T): DataSourceSettings<LokiOptions> => {
    return {
      ...options,
      jsonData: {
        ...options.jsonData,
        [field]: value,
      },
    };
  };

const setMaxLines = makeJsonUpdater('maxLines');
const setPredefinedOperations = makeJsonUpdater('predefinedOperations');
const setDerivedFields = makeJsonUpdater('derivedFields');

export const ConfigEditor = (props: Props) => {
  const { options, onOptionsChange } = props;

  const updatePredefinedOperations = useCallback(
    (value: string) => {
      reportInteraction('grafana_loki_predefined_operations_changed', { value });
      onOptionsChange(setPredefinedOperations(options, value));
    },
    [options, onOptionsChange]
  );

  return (
    <>
      <DataSourceDescription
        dataSourceName="Loki"
        docsLink="https://grafana.com/docs/grafana/latest/datasources/loki"
        hasRequiredFields={false}
      />

      <Divider />

      <DataSourceHttpSettings
        defaultUrl={'http://localhost:3100'}
        dataSourceConfig={options}
        showAccessOptions={false}
        onChange={onOptionsChange}
        secureSocksDSProxyEnabled={config.secureSocksDSProxyEnabled}
      />

      <Divider />

      <ConfigSection
        title="Additional settings"
        description="Additional settings are optional settings that can be configured for more control over your data source."
        isCollapsible={true}
        isInitiallyOpen
      >
        <AlertingSettings options={options} onOptionsChange={onOptionsChange} />
        <Divider hideLine />
        <QuerySettings
          maxLines={options.jsonData.maxLines || ''}
          onMaxLinedChange={(value) => onOptionsChange(setMaxLines(options, value))}
          predefinedOperations={options.jsonData.predefinedOperations || ''}
          onPredefinedOperationsChange={updatePredefinedOperations}
        />
        <Divider hideLine />
        <DerivedFields
          fields={options.jsonData.derivedFields}
          onChange={(value) => onOptionsChange(setDerivedFields(options, value))}
        />
      </ConfigSection>
    </>
  );
};
