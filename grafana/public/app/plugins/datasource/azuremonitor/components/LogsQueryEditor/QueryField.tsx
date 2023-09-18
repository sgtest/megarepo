import { EngineSchema, Schema } from '@kusto/monaco-kusto';
import { Uri } from 'monaco-editor';
import React, { useCallback, useEffect, useState } from 'react';

import { CodeEditor, Monaco, MonacoEditor } from '@grafana/ui';

import { AzureQueryEditorFieldProps } from '../../types';

import { setKustoQuery } from './setQueryValue';

interface MonacoEditorValues {
  editor: MonacoEditor;
  monaco: Monaco;
}

interface MonacoLanguages {
  kusto: {
    getKustoWorker: () => Promise<
      (url: Uri) => Promise<{
        setSchema: (schema: Schema) => void;
      }>
    >;
  };
}

const QueryField = ({ query, onQueryChange, schema }: AzureQueryEditorFieldProps) => {
  const [monaco, setMonaco] = useState<MonacoEditorValues | undefined>();

  useEffect(() => {
    if (!schema || !monaco) {
      return;
    }

    const setupEditor = async ({ monaco, editor }: MonacoEditorValues, schema: EngineSchema) => {
      try {
        const languages = monaco.languages as unknown as MonacoLanguages;
        const model = editor.getModel();
        if (model) {
          const kustoWorker = await languages.kusto.getKustoWorker();
          const kustoMode = await kustoWorker(model?.uri);
          await kustoMode.setSchema(schema);
        }
      } catch (err) {
        console.error(err);
      }
    };

    setupEditor(monaco, schema).catch((err) => console.error(err));
  }, [schema, monaco]);

  const handleEditorMount = useCallback((editor: MonacoEditor, monaco: Monaco) => {
    setMonaco({ monaco, editor });
  }, []);

  const onChange = useCallback(
    (newQuery: string) => {
      onQueryChange(setKustoQuery(query, newQuery));
    },
    [onQueryChange, query]
  );

  return (
    <CodeEditor
      value={query.azureLogAnalytics?.query ?? ''}
      language="kusto"
      height={200}
      width="100%"
      showMiniMap={false}
      onBlur={onChange}
      onSave={onChange}
      onEditorDidMount={handleEditorMount}
    />
  );
};

export default QueryField;
