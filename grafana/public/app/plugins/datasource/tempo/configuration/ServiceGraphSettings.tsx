import React from 'react';

import {
  DataSourceInstanceSettings,
  DataSourcePluginOptionsEditorProps,
  updateDatasourcePluginJsonDataOption,
} from '@grafana/data';
import { Button, InlineField, InlineFieldRow, useStyles2 } from '@grafana/ui';
import { DataSourcePicker } from 'app/features/datasources/components/picker/DataSourcePicker';

import { TempoJsonData } from '../types';

import { getStyles } from './QuerySettings';

interface Props extends DataSourcePluginOptionsEditorProps<TempoJsonData> {}

export function ServiceGraphSettings({ options, onOptionsChange }: Props) {
  const styles = useStyles2(getStyles);

  return (
    <div className={styles.container}>
      <InlineFieldRow className={styles.row}>
        <InlineField
          tooltip="The Prometheus data source with the service graph data"
          label="Data source"
          labelWidth={26}
        >
          <DataSourcePicker
            inputId="service-graph-data-source-picker"
            pluginId="prometheus"
            current={options.jsonData.serviceMap?.datasourceUid}
            noDefault={true}
            width={40}
            onChange={(ds: DataSourceInstanceSettings) =>
              updateDatasourcePluginJsonDataOption({ onOptionsChange, options }, 'serviceMap', {
                datasourceUid: ds.uid,
              })
            }
          />
        </InlineField>
        {options.jsonData.serviceMap?.datasourceUid ? (
          <Button
            type={'button'}
            variant={'secondary'}
            size={'sm'}
            fill={'text'}
            onClick={() => {
              updateDatasourcePluginJsonDataOption({ onOptionsChange, options }, 'serviceMap', {
                datasourceUid: undefined,
              });
            }}
          >
            Clear
          </Button>
        ) : null}
      </InlineFieldRow>
    </div>
  );
}
