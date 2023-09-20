import { Meta, StoryFn } from '@storybook/react';
import React from 'react';

import { Button } from '../Button';
import mdx from '../Tooltip/Tooltip.mdx';

import { Tooltip } from './Tooltip';

const meta: Meta<typeof Tooltip> = {
  title: 'Overlays/Tooltip',
  component: Tooltip,
  parameters: {
    docs: {
      page: mdx,
    },
    knobs: {
      disabled: true,
    },
  },
  argTypes: {
    content: {
      control: {
        type: 'text',
      },
    },
    theme: {
      control: {
        type: 'select',
        options: ['info', 'error'],
      },
    },
    placement: {
      control: {
        type: 'select',
        options: [
          'auto',
          'bottom',
          'top',
          'auto-start',
          'auto-end',
          'right',
          'left',
          'top-start',
          'top-end',
          'bottom-start',
          'bottom-end',
          'right-start',
          'right-end',
          'left-start',
          'left-end',
        ],
      },
    },
  },
};

export const Basic: StoryFn<typeof Tooltip> = ({ content, ...args }) => {
  return (
    <Tooltip content={content} {...args}>
      <Button>Hover me for Tooltip </Button>
    </Tooltip>
  );
};

Basic.args = {
  content: 'This is a tooltip',
  theme: 'info',
  show: undefined,
  placement: 'auto',
};

export default meta;
