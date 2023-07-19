// Copyright (c) 2019 Uber Technologies, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { SpanStatusCode } from '@opentelemetry/api';

import { SearchProps, Tag } from '../../useSearch';
import { KIND, LIBRARY_NAME, LIBRARY_VERSION, STATUS, STATUS_MESSAGE, TRACE_STATE, ID } from '../constants/span';
import { TNil, TraceKeyValuePair, TraceSpan } from '../types';

// filter spans where all filters added need to be true for each individual span that is returned
// i.e. the more filters added -> the more specific that the returned results are
export function filterSpans(searchProps: SearchProps, spans: TraceSpan[] | TNil) {
  if (!spans) {
    return undefined;
  }

  let filteredSpans = false;
  if (searchProps.serviceName) {
    spans = getServiceNameMatches(spans, searchProps);
    filteredSpans = true;
  }
  if (searchProps.spanName) {
    spans = getSpanNameMatches(spans, searchProps);
    filteredSpans = true;
  }
  if (searchProps.from || searchProps.to) {
    spans = getDurationMatches(spans, searchProps);
    filteredSpans = true;
  }
  const tagMatches = getTagMatches(spans, searchProps.tags);
  if (tagMatches) {
    spans = tagMatches;
    filteredSpans = true;
  }

  return filteredSpans ? new Set(spans.map((span: TraceSpan) => span.spanID)) : undefined;
}

const getTagMatches = (spans: TraceSpan[], tags: Tag[]) => {
  // remove empty/default tags
  tags = tags.filter((tag) => {
    // tag.key === '' when it is cleared via pressing x icon in select field
    return (tag.key && tag.key !== '') || tag.value;
  });

  if (tags.length > 0) {
    return spans.filter((span: TraceSpan) => {
      // match against every tag filter
      return tags.every((tag: Tag) => {
        if (tag.key && tag.value) {
          if (
            span.tags.some((kv) => checkKeyAndValueForMatch(tag, kv)) ||
            span.process.tags.some((kv) => checkKeyAndValueForMatch(tag, kv)) ||
            (span.logs && span.logs.some((log) => log.fields.some((kv) => checkKeyAndValueForMatch(tag, kv)))) ||
            (span.kind && tag.key === KIND && tag.value === span.kind) ||
            (span.statusCode !== undefined &&
              tag.key === STATUS &&
              tag.value === SpanStatusCode[span.statusCode].toLowerCase()) ||
            (span.statusMessage && tag.key === STATUS_MESSAGE && tag.value === span.statusMessage) ||
            (span.instrumentationLibraryName &&
              tag.key === LIBRARY_NAME &&
              tag.value === span.instrumentationLibraryName) ||
            (span.instrumentationLibraryVersion &&
              tag.key === LIBRARY_VERSION &&
              tag.value === span.instrumentationLibraryVersion) ||
            (span.traceState && tag.key === TRACE_STATE && tag.value === span.traceState) ||
            (tag.key === ID && tag.value === span.spanID)
          ) {
            return getReturnValue(tag.operator, true);
          }
        } else if (tag.key) {
          if (
            span.tags.some((kv) => checkKeyForMatch(tag.key!, kv.key)) ||
            span.process.tags.some((kv) => checkKeyForMatch(tag.key!, kv.key)) ||
            (span.logs && span.logs.some((log) => log.fields.some((kv) => checkKeyForMatch(tag.key!, kv.key)))) ||
            (span.kind && tag.key === KIND) ||
            (span.statusCode !== undefined && tag.key === STATUS) ||
            (span.statusMessage && tag.key === STATUS_MESSAGE) ||
            (span.instrumentationLibraryName && tag.key === LIBRARY_NAME) ||
            (span.instrumentationLibraryVersion && tag.key === LIBRARY_VERSION) ||
            (span.traceState && tag.key === TRACE_STATE) ||
            tag.key === ID
          ) {
            return getReturnValue(tag.operator, true);
          }
        }
        return getReturnValue(tag.operator, false);
      });
    });
  }
  return undefined;
};

const checkKeyForMatch = (tagKey: string, key: string) => {
  return tagKey === key.toString() ? true : false;
};

const checkKeyAndValueForMatch = (tag: Tag, kv: TraceKeyValuePair) => {
  return tag.key === kv.key.toString() && tag.value === kv.value.toString() ? true : false;
};

const getReturnValue = (operator: string, found: boolean) => {
  return operator === '=' ? found : !found;
};

const getServiceNameMatches = (spans: TraceSpan[], searchProps: SearchProps) => {
  return spans.filter((span: TraceSpan) => {
    return searchProps.serviceNameOperator === '='
      ? span.process.serviceName === searchProps.serviceName
      : span.process.serviceName !== searchProps.serviceName;
  });
};

const getSpanNameMatches = (spans: TraceSpan[], searchProps: SearchProps) => {
  return spans.filter((span: TraceSpan) => {
    return searchProps.spanNameOperator === '='
      ? span.operationName === searchProps.spanName
      : span.operationName !== searchProps.spanName;
  });
};

const getDurationMatches = (spans: TraceSpan[], searchProps: SearchProps) => {
  const from = convertTimeFilter(searchProps?.from || '');
  const to = convertTimeFilter(searchProps?.to || '');
  let filteredSpans: TraceSpan[] = [];

  if (from) {
    filteredSpans = spans.filter((span: TraceSpan) => {
      return searchProps.fromOperator === '>' ? span.duration > from : span.duration >= from;
    });
  }
  if (to) {
    const filterForDuration = (span: TraceSpan) =>
      searchProps.toOperator === '<' ? span.duration < to : span.duration <= to;
    filteredSpans =
      filteredSpans.length > 0
        ? filteredSpans.filter((span: TraceSpan) => {
            return filterForDuration(span);
          })
        : spans.filter((span: TraceSpan) => {
            return filterForDuration(span);
          });
  }

  return filteredSpans;
};

export const convertTimeFilter = (time: string) => {
  if (time.includes('ns')) {
    return parseFloat(time.split('ns')[0]) / 1000;
  } else if (time.includes('us')) {
    return parseFloat(time.split('us')[0]);
  } else if (time.includes('µs')) {
    return parseFloat(time.split('µs')[0]);
  } else if (time.includes('ms')) {
    return parseFloat(time.split('ms')[0]) * 1000;
  } else if (time.includes('s')) {
    return parseFloat(time.split('s')[0]) * 1000 * 1000;
  } else if (time.includes('m')) {
    return parseFloat(time.split('m')[0]) * 1000 * 1000 * 60;
  } else if (time.includes('h')) {
    return parseFloat(time.split('h')[0]) * 1000 * 1000 * 60 * 60;
  }
  return undefined;
};
