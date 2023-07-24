import { css } from '@emotion/css';
import React from 'react';

import { QueryEditorProps, SelectableValue } from '@grafana/data';
import { config, reportInteraction } from '@grafana/runtime';
import {
  Button,
  FileDropzone,
  HorizontalGroup,
  InlineField,
  InlineFieldRow,
  Modal,
  RadioButtonGroup,
  Themeable2,
  withTheme2,
} from '@grafana/ui';

import { LokiQuery } from '../loki/types';

import { LokiSearch } from './LokiSearch';
import NativeSearch from './NativeSearch/NativeSearch';
import TraceQLSearch from './SearchTraceQLEditor/TraceQLSearch';
import { ServiceGraphSection } from './ServiceGraphSection';
import { TempoQueryType } from './dataquery.gen';
import { TempoDatasource } from './datasource';
import { QueryEditor } from './traceql/QueryEditor';
import { TempoQuery } from './types';

interface Props extends QueryEditorProps<TempoDatasource, TempoQuery>, Themeable2 {}
interface State {
  uploadModalOpen: boolean;
}

const DEFAULT_QUERY_TYPE: TempoQueryType = 'traceqlSearch';

class TempoQueryFieldComponent extends React.PureComponent<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = {
      uploadModalOpen: false,
    };
  }

  // Set the default query type when the component mounts.
  // Also do this if queryType is 'clear' (which is the case when the user changes the query type)
  // otherwise if the user changes the query type and refreshes the page, no query type will be selected
  // which is inconsistent with how the UI was originally when they selected the Tempo data source.
  async componentDidMount() {
    if (!this.props.query.queryType || this.props.query.queryType === 'clear') {
      this.props.onChange({
        ...this.props.query,
        queryType: DEFAULT_QUERY_TYPE,
      });
    }
  }

  onChangeLinkedQuery = (value: LokiQuery) => {
    const { query, onChange } = this.props;
    onChange({
      ...query,
      linkedQuery: { ...value, refId: 'linked' },
    });
  };

  onRunLinkedQuery = () => {
    this.props.onRunQuery();
  };

  onClearResults = () => {
    // Run clear query to clear results
    const { onChange, query, onRunQuery } = this.props;
    onChange({
      ...query,
      queryType: 'clear',
    });
    onRunQuery();
  };

  render() {
    const { query, onChange, datasource, app } = this.props;

    const logsDatasourceUid = datasource.getLokiSearchDS();

    const graphDatasourceUid = datasource.serviceMap?.datasourceUid;

    let queryTypeOptions: Array<SelectableValue<TempoQueryType>> = [
      { value: 'traceqlSearch', label: 'Search' },
      { value: 'traceql', label: 'TraceQL' },
      { value: 'serviceMap', label: 'Service Graph' },
    ];

    if (logsDatasourceUid) {
      if (datasource?.search?.hide) {
        // Place at beginning as Search if no native search
        queryTypeOptions.unshift({ value: 'search', label: 'Search' });
      } else {
        // Place at end as Loki Search if native search is enabled
        queryTypeOptions.push({ value: 'search', label: 'Loki Search' });
      }
    }

    // Show the deprecated search option if any of the deprecated search fields are set
    if (
      query.spanName ||
      query.serviceName ||
      query.search ||
      query.maxDuration ||
      query.minDuration ||
      query.queryType === 'nativeSearch'
    ) {
      queryTypeOptions.unshift({ value: 'nativeSearch', label: '[Deprecated] Search' });
    }

    return (
      <>
        <Modal
          title={'Upload trace'}
          isOpen={this.state.uploadModalOpen}
          onDismiss={() => this.setState({ uploadModalOpen: false })}
        >
          <div className={css({ padding: this.props.theme.spacing(2) })}>
            <FileDropzone
              options={{ multiple: false }}
              onLoad={(result) => {
                this.props.datasource.uploadedJson = result;
                onChange({
                  ...query,
                  queryType: 'upload',
                });
                this.setState({ uploadModalOpen: false });
                this.props.onRunQuery();
              }}
            />
          </div>
        </Modal>
        <InlineFieldRow>
          <InlineField label="Query type" grow={true}>
            <HorizontalGroup spacing={'sm'} align={'center'} justify={'space-between'}>
              <RadioButtonGroup<TempoQueryType>
                options={queryTypeOptions}
                value={query.queryType}
                onChange={(v) => {
                  reportInteraction('grafana_traces_query_type_changed', {
                    datasourceType: 'tempo',
                    app: app ?? '',
                    grafana_version: config.buildInfo.version,
                    newQueryType: v,
                    previousQueryType: query.queryType ?? '',
                  });

                  this.onClearResults();

                  onChange({
                    ...query,
                    queryType: v,
                  });
                }}
                size="md"
              />
              <Button
                variant="secondary"
                size="sm"
                onClick={() => {
                  this.setState({ uploadModalOpen: true });
                }}
              >
                Import trace
              </Button>
            </HorizontalGroup>
          </InlineField>
        </InlineFieldRow>
        {query.queryType === 'search' && (
          <LokiSearch
            logsDatasourceUid={logsDatasourceUid}
            query={query}
            onRunQuery={this.onRunLinkedQuery}
            onChange={this.onChangeLinkedQuery}
          />
        )}
        {query.queryType === 'nativeSearch' && (
          <NativeSearch
            datasource={this.props.datasource}
            query={query}
            onChange={onChange}
            onBlur={this.props.onBlur}
            onRunQuery={this.props.onRunQuery}
          />
        )}
        {query.queryType === 'traceqlSearch' && (
          <TraceQLSearch
            datasource={this.props.datasource}
            query={query}
            onChange={onChange}
            onBlur={this.props.onBlur}
          />
        )}
        {query.queryType === 'serviceMap' && (
          <ServiceGraphSection graphDatasourceUid={graphDatasourceUid} query={query} onChange={onChange} />
        )}
        {query.queryType === 'traceql' && (
          <QueryEditor
            datasource={this.props.datasource}
            query={query}
            onRunQuery={this.props.onRunQuery}
            onChange={onChange}
          />
        )}
      </>
    );
  }
}

export const TempoQueryField = withTheme2(TempoQueryFieldComponent);
