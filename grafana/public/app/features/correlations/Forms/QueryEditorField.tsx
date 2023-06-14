import React from 'react';
import { Controller } from 'react-hook-form';
import { useAsync } from 'react-use';

import { CoreApp } from '@grafana/data';
import { getDataSourceSrv } from '@grafana/runtime';
import { Field, LoadingPlaceholder, Alert } from '@grafana/ui';

interface Props {
  dsUid?: string;
  name: string;
  invalid?: boolean;
  error?: string;
}

export const QueryEditorField = ({ dsUid, invalid, error, name }: Props) => {
  const {
    value: datasource,
    loading: dsLoading,
    error: dsError,
  } = useAsync(async () => {
    if (!dsUid) {
      return;
    }
    return getDataSourceSrv().get(dsUid);
  }, [dsUid]);

  const QueryEditor = datasource?.components?.QueryEditor;

  return (
    <Field
      label="Query"
      description={
        <span>
          Define the query that is run when the link is clicked. You can use{' '}
          <a
            href="https://grafana.com/docs/grafana/latest/panels-visualizations/configure-data-links/"
            target="_blank"
            rel="noreferrer"
          >
            variables
          </a>{' '}
          to access specific field values.
        </span>
      }
      invalid={invalid}
      error={error}
    >
      <Controller
        name={name}
        rules={{
          validate: {
            hasQueryEditor: () =>
              QueryEditor !== undefined || 'The selected target data source must export a query editor.',
          },
        }}
        render={({ field: { value, onChange } }) => {
          if (dsLoading) {
            return <LoadingPlaceholder text="Loading query editor..." />;
          }
          if (dsError) {
            return <Alert title="Error loading data source">The selected data source could not be loaded.</Alert>;
          }
          if (!datasource) {
            return (
              <Alert title="No data source selected" severity="info">
                Please select a target data source first.
              </Alert>
            );
          }
          if (!QueryEditor) {
            return <Alert title="Data source does not export a query editor."></Alert>;
          }
          return (
            <>
              <QueryEditor
                onRunQuery={() => {}}
                app={CoreApp.Correlations}
                onChange={(value) => {
                  onChange(value);
                }}
                datasource={datasource}
                query={value}
              />
            </>
          );
        }}
      />
    </Field>
  );
};
