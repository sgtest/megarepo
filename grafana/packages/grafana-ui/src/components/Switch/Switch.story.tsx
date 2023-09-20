import { Meta, StoryFn } from '@storybook/react';
import React, { useState, useCallback } from 'react';

import { InlineField, Switch, InlineSwitch } from '@grafana/ui';

import { Field } from '../Forms/Field';
import { InlineFieldRow } from '../Forms/InlineFieldRow';

import mdx from './Switch.mdx';

const meta: Meta<typeof Switch> = {
  title: 'Forms/Switch',
  component: Switch,
  parameters: {
    docs: {
      page: mdx,
    },
  },
  args: {
    disabled: false,
    value: false,
    transparent: false,
    invalid: false,
  },
};

export const Controlled: StoryFn<typeof Switch> = (args) => {
  return (
    <div>
      <div style={{ marginBottom: '32px' }}>
        <Field label="Normal switch" description="For horizontal forms" invalid={args.invalid}>
          <Switch value={args.value} disabled={args.disabled} transparent={args.transparent} />
        </Field>
      </div>
      <div style={{ marginBottom: '32px' }}>
        <InlineFieldRow>
          <InlineField label="My switch" invalid={args.invalid}>
            <InlineSwitch value={args.value} disabled={args.disabled} transparent={args.transparent} />
          </InlineField>
        </InlineFieldRow>
      </div>
      <div style={{ marginBottom: '32px' }}>
        <div>just inline switch with show label</div>
        <span>
          <InlineSwitch
            label="Raw data"
            showLabel={true}
            value={args.value}
            disabled={args.disabled}
            transparent={args.transparent}
            invalid={args.invalid}
          />
        </span>
      </div>
    </div>
  );
};

export const Uncontrolled: StoryFn<typeof Switch> = (args) => {
  const [checked, setChecked] = useState(args.value);
  const onChange = useCallback(
    (e: React.FormEvent<HTMLInputElement>) => setChecked(e.currentTarget.checked),
    [setChecked]
  );
  return (
    <Switch
      value={checked}
      disabled={args.disabled}
      transparent={args.transparent}
      onChange={onChange}
      invalid={args.invalid}
    />
  );
};

export default meta;
