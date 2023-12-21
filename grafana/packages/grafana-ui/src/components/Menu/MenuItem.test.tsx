import { fireEvent, render, screen } from '@testing-library/react';
import React from 'react';

import { selectors } from '@grafana/e2e-selectors';

import { MenuItem, MenuItemProps } from './MenuItem';

describe('MenuItem', () => {
  const getMenuItem = (props?: Partial<MenuItemProps>) => (
    <MenuItem ariaLabel={selectors.components.Menu.MenuItem('Test')} label="item1" icon="history" {...props} />
  );

  it('renders correct element type', () => {
    const { rerender } = render(getMenuItem({ onClick: jest.fn() }));

    expect(screen.getByLabelText(selectors.components.Menu.MenuItem('Test')).nodeName).toBe('BUTTON');

    rerender(getMenuItem({ url: 'test' }));

    expect(screen.getByLabelText(selectors.components.Menu.MenuItem('Test')).nodeName).toBe('A');
  });

  it('calls onClick when item is clicked', () => {
    const onClick = jest.fn();

    render(getMenuItem({ onClick }));

    fireEvent.click(screen.getByLabelText(selectors.components.Menu.MenuItem('Test')));

    expect(onClick).toHaveBeenCalled();
  });

  it('renders and opens subMenu correctly', async () => {
    const childItems = [
      <MenuItem key="subitem1" label="subitem1" icon="history" />,
      <MenuItem key="subitem2" label="subitem2" icon="apps" />,
    ];

    render(getMenuItem({ childItems }));

    expect(screen.getByLabelText(selectors.components.Menu.MenuItem('Test')).nodeName).toBe('DIV');
    expect(screen.getByTestId(selectors.components.Menu.SubMenu.icon)).toBeInTheDocument();
    expect(screen.queryByTestId(selectors.components.Menu.SubMenu.container)).not.toBeInTheDocument();

    fireEvent.mouseOver(screen.getByLabelText(selectors.components.Menu.MenuItem('Test')));

    const subMenuContainer = await screen.findByTestId(selectors.components.Menu.SubMenu.container);

    expect(subMenuContainer).toBeInTheDocument();
    expect(subMenuContainer.firstChild?.childNodes.length).toBe(2);
  });

  it('renders disabled subMenu correctly', async () => {
    const childItems = [
      <MenuItem key="subitem1" label="subitem1" icon="history" />,
      <MenuItem key="subitem2" label="subitem2" icon="apps" />,
    ];

    render(getMenuItem({ childItems, disabled: true }));

    fireEvent.mouseOver(screen.getByLabelText(selectors.components.Menu.MenuItem('Test')));

    const subMenuContainer = screen.queryByLabelText(selectors.components.Menu.SubMenu.container);
    expect(subMenuContainer).toBe(null);
  });

  it('opens subMenu on ArrowRight', async () => {
    const childItems = [
      <MenuItem key="subitem1" label="subitem1" icon="history" />,
      <MenuItem key="subitem2" label="subitem2" icon="apps" />,
    ];

    render(getMenuItem({ childItems }));

    expect(screen.queryByTestId(selectors.components.Menu.SubMenu.container)).not.toBeInTheDocument();

    fireEvent.keyDown(screen.getByLabelText(selectors.components.Menu.MenuItem('Test')), { key: 'ArrowRight' });

    expect(await screen.findByTestId(selectors.components.Menu.SubMenu.container)).toBeInTheDocument();
  });
});
