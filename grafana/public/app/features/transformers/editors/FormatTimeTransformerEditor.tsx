import React, { useCallback, ChangeEvent } from 'react';

import {
  DataTransformerID,
  SelectableValue,
  standardTransformers,
  TransformerRegistryItem,
  TransformerUIProps,
  getFieldDisplayName,
  PluginState,
} from '@grafana/data';
import { FormatTimeTransformerOptions } from '@grafana/data/src/transformations/transformers/formatTime';
import { Select, InlineFieldRow, InlineField, Input, InlineSwitch } from '@grafana/ui';

export function FormatTimeTransfomerEditor({
  input,
  options,
  onChange,
}: TransformerUIProps<FormatTimeTransformerOptions>) {
  const timeFields: Array<SelectableValue<string>> = [];

  // Get time fields
  for (const frame of input) {
    for (const field of frame.fields) {
      if (field.type === 'time') {
        const name = getFieldDisplayName(field, frame, input);
        timeFields.push({ label: name, value: name });
      }
    }
  }

  const onSelectField = useCallback(
    (value: SelectableValue<string>) => {
      const val = value?.value !== undefined ? value.value : '';
      onChange({
        ...options,
        timeField: val,
      });
    },
    [onChange, options]
  );

  const onFormatChange = useCallback(
    (e: ChangeEvent<HTMLInputElement>) => {
      const val = e.target.value;
      onChange({
        ...options,
        outputFormat: val,
      });
    },
    [onChange, options]
  );

  const onUseTzChange = useCallback(() => {
    onChange({
      ...options,
      useTimezone: !options.useTimezone,
    });
  }, [onChange, options]);

  return (
    <>
      <InlineFieldRow>
        <InlineField label="Time Field" labelWidth={15} grow>
          <Select
            options={timeFields}
            value={options.timeField}
            onChange={onSelectField}
            placeholder="time"
            isClearable
          />
        </InlineField>

        <InlineField
          label="Format"
          labelWidth={10}
          tooltip="The output format for the field specified as a moment.js format string."
        >
          <Input onChange={onFormatChange} value={options.outputFormat} />
        </InlineField>
        <InlineField
          label="Use Timezone"
          tooltip="Use the user's configured timezone when formatting time."
          labelWidth={20}
        >
          <InlineSwitch value={options.useTimezone} transparent={true} onChange={onUseTzChange} />
        </InlineField>
      </InlineFieldRow>
    </>
  );
}

export const formatTimeTransformerRegistryItem: TransformerRegistryItem<FormatTimeTransformerOptions> = {
  id: DataTransformerID.formatTime,
  editor: FormatTimeTransfomerEditor,
  transformation: standardTransformers.formatTimeTransformer,
  name: standardTransformers.formatTimeTransformer.name,
  state: PluginState.alpha,
  description: standardTransformers.formatTimeTransformer.description,
};
