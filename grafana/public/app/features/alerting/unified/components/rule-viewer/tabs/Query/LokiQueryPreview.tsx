import React from 'react';

import { RawQuery } from '@grafana/experimental';
import lokiGrammar from 'app/plugins/datasource/loki/syntax';

interface Props {
  query: string;
}

const LokiQueryPreview = ({ query }: Props) => {
  return <RawQuery query={query} language={{ grammar: lokiGrammar, name: 'promql' }} />;
};

export default LokiQueryPreview;
