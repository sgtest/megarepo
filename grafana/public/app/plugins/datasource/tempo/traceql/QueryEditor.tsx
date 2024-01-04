import { css } from '@emotion/css';
import { defaults } from 'lodash';
import React, { useState } from 'react';

import { QueryEditorProps } from '@grafana/data';
import { config, reportInteraction } from '@grafana/runtime';
import { Button, InlineLabel, useStyles2 } from '@grafana/ui';

import { generateQueryFromFilters } from '../SearchTraceQLEditor/utils';
import { TempoDatasource } from '../datasource';
import { defaultQuery, MyDataSourceOptions, TempoQuery } from '../types';

import { TempoQueryBuilderOptions } from './TempoQueryBuilderOptions';
import { TraceQLEditor } from './TraceQLEditor';

type EditorProps = {
  onClearResults: () => void;
};

type Props = EditorProps & QueryEditorProps<TempoDatasource, TempoQuery, MyDataSourceOptions>;

export function QueryEditor(props: Props) {
  const styles = useStyles2(getStyles);
  const query = defaults(props.query, defaultQuery);
  const [showCopyFromSearchButton, setShowCopyFromSearchButton] = useState(() => {
    const genQuery = generateQueryFromFilters(query.filters || []);
    return genQuery === query.query || genQuery === '{}';
  });

  const onEditorChange = (value: string) => {
    props.onChange({ ...query, query: value });
  };

  return (
    <>
      <InlineLabel>
        Build complex queries using TraceQL to select a list of traces.{' '}
        <a rel="noreferrer" target="_blank" href="https://grafana.com/docs/tempo/latest/traceql/">
          Documentation
        </a>
      </InlineLabel>
      {!showCopyFromSearchButton && (
        <InlineLabel>
          <div>
            Continue editing the query from the Search tab?
            <Button
              variant="secondary"
              size="sm"
              onClick={() => {
                reportInteraction('grafana_traces_copy_to_traceql_clicked', {
                  app: props.app ?? '',
                  grafana_version: config.buildInfo.version,
                  location: 'traceql_tab',
                });

                props.onClearResults();
                props.onChange({
                  ...query,
                  query: generateQueryFromFilters(query.filters || []),
                });
                setShowCopyFromSearchButton(true);
              }}
              style={{ marginLeft: '10px' }}
            >
              Copy query from Search
            </Button>
          </div>
        </InlineLabel>
      )}
      <TraceQLEditor
        placeholder="Enter a TraceQL query or trace ID (run with Shift+Enter)"
        value={query.query || ''}
        onChange={onEditorChange}
        datasource={props.datasource}
        onRunQuery={props.onRunQuery}
      />
      <div className={styles.optionsContainer}>
        <TempoQueryBuilderOptions query={query} onChange={props.onChange} />
      </div>
    </>
  );
}

const getStyles = () => ({
  optionsContainer: css`
    margin-top: 10px;
  `,
});
