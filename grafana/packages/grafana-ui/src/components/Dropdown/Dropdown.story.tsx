import { Meta } from '@storybook/react';
import React from 'react';

import { StoryExample } from '../../utils/storybook/StoryExample';
import { Button } from '../Button';
import { IconButton } from '../IconButton/IconButton';
import { VerticalGroup } from '../Layout/Layout';
import { Menu } from '../Menu/Menu';

import { Dropdown } from './Dropdown';
import mdx from './Dropdown.mdx';

const meta: Meta<typeof Dropdown> = {
  title: 'Overlays/Dropdown',
  component: Dropdown,
  parameters: {
    docs: {
      page: mdx,
    },
    controls: {
      exclude: ['className'],
    },
  },
};

export function Examples() {
  const menu = (
    <Menu>
      <Menu.Item label="View settings" />
      <Menu.Item label="Edit actions" />
      <Menu.Item label="Share" />
      <Menu.Item label="Delete" />
    </Menu>
  );

  return (
    <VerticalGroup>
      <StoryExample name="Button + defaults">
        <Dropdown overlay={menu}>
          <Button variant="secondary">Button</Button>
        </Dropdown>
      </StoryExample>

      <StoryExample name="Icon button, placement=bottom-start">
        <Dropdown overlay={menu} placement="bottom-start">
          <IconButton tooltip="Open menu" variant="secondary" name="bars" />
        </Dropdown>
      </StoryExample>
    </VerticalGroup>
  );
}

Examples.parameters = {
  controls: {
    hideNoControlsWarning: true,
    include: [],
  },
};

export default meta;
