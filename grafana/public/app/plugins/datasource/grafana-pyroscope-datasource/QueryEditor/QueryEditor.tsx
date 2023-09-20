import deepEqual from 'fast-deep-equal';
import React, { useCallback, useEffect } from 'react';
import { useAsync } from 'react-use';

import { CoreApp, QueryEditorProps, TimeRange } from '@grafana/data';
import { LoadingPlaceholder } from '@grafana/ui';

import { normalizeQuery, PyroscopeDataSource } from '../datasource';
import { PyroscopeDataSourceOptions, ProfileTypeMessage, Query } from '../types';

import { EditorRow } from './EditorRow';
import { EditorRows } from './EditorRows';
import { LabelsEditor } from './LabelsEditor';
import { ProfileTypesCascader, useProfileTypes } from './ProfileTypesCascader';
import { QueryOptions } from './QueryOptions';

export type Props = QueryEditorProps<PyroscopeDataSource, Query, PyroscopeDataSourceOptions>;

export function QueryEditor(props: Props) {
  const { onChange, onRunQuery, datasource, query, range, app } = props;

  function handleRunQuery(value: string) {
    onChange({ ...query, labelSelector: value });
    onRunQuery();
  }

  const profileTypes = useProfileTypes(datasource);
  const { labels, getLabelValues, onLabelSelectorChange } = useLabels(range, datasource, query, onChange);
  useNormalizeQuery(query, profileTypes, onChange, app);

  let cascader = <LoadingPlaceholder text={'Loading'} />;

  // The cascader is uncontrolled component so if we want to set some default value we can do it only on initial
  // render, so we are waiting until we have the profileTypes and know what the default value should be before
  // rendering.
  if (profileTypes && query.profileTypeId !== undefined) {
    cascader = (
      <ProfileTypesCascader
        placeholder={profileTypes.length === 0 ? 'No profile types found' : 'Select profile type'}
        profileTypes={profileTypes}
        initialProfileTypeId={query.profileTypeId}
        onChange={(val) => {
          onChange({ ...query, profileTypeId: val });
        }}
      />
    );
  }

  return (
    <EditorRows>
      <EditorRow stackProps={{ wrap: false, gap: 1 }}>
        {cascader}
        <LabelsEditor
          value={query.labelSelector}
          onChange={onLabelSelectorChange}
          onRunQuery={handleRunQuery}
          labels={labels}
          getLabelValues={getLabelValues}
        />
      </EditorRow>
      <EditorRow>
        <QueryOptions query={query} onQueryChange={props.onChange} app={props.app} labels={labels} />
      </EditorRow>
    </EditorRows>
  );
}

function useNormalizeQuery(
  query: Query,
  profileTypes: ProfileTypeMessage[] | undefined,
  onChange: (value: Query) => void,
  app?: CoreApp
) {
  useEffect(() => {
    if (!profileTypes) {
      return;
    }
    const normalizedQuery = normalizeQuery(query, app);
    // We just check if profileTypeId is filled but don't check if it's one of the existing cause it can be template
    // variable
    if (!query.profileTypeId) {
      normalizedQuery.profileTypeId = defaultProfileType(profileTypes);
    }
    // Makes sure we don't have an infinite loop updates because the normalization creates a new object
    if (!deepEqual(query, normalizedQuery)) {
      onChange(normalizedQuery);
    }
  }, [app, query, profileTypes, onChange]);
}

function defaultProfileType(profileTypes: ProfileTypeMessage[]): string {
  const cpuProfiles = profileTypes.filter((p) => p.id.indexOf('cpu') >= 0);
  if (cpuProfiles.length) {
    // Prefer cpu time profile if available instead of samples
    const cpuTimeProfile = cpuProfiles.find((p) => p.id.indexOf('samples') === -1);
    if (cpuTimeProfile) {
      return cpuTimeProfile.id;
    }
    // Fallback to first cpu profile type
    return cpuProfiles[0].id;
  }

  // Fallback to first profile type from response data
  return profileTypes[0]?.id || '';
}

function useLabels(
  range: TimeRange | undefined,
  datasource: PyroscopeDataSource,
  query: Query,
  onChange: (value: Query) => void
) {
  // Round to nearest 5 seconds. If the range is something like last 1h then every render the range values change slightly
  // and what ever has range as dependency is rerun. So this effectively debounces the queries.
  const unpreciseRange = {
    to: Math.ceil((range?.to.valueOf() || 0) / 5000) * 5000,
    from: Math.floor((range?.from.valueOf() || 0) / 5000) * 5000,
  };

  const labelsResult = useAsync(() => {
    return datasource.getLabelNames(query.profileTypeId + query.labelSelector, unpreciseRange.from, unpreciseRange.to);
  }, [datasource, query.profileTypeId, query.labelSelector, unpreciseRange.to, unpreciseRange.from]);

  // Create a function with range and query already baked in so we don't have to send those everywhere
  const getLabelValues = useCallback(
    (label: string) => {
      return datasource.getLabelValues(
        query.profileTypeId + query.labelSelector,
        label,
        unpreciseRange.from,
        unpreciseRange.to
      );
    },
    [query, datasource, unpreciseRange.to, unpreciseRange.from]
  );

  const onLabelSelectorChange = useCallback(
    (value: string) => {
      onChange({ ...query, labelSelector: value });
    },
    [onChange, query]
  );

  return { labels: labelsResult.value, getLabelValues, onLabelSelectorChange };
}
