import { Store } from 'redux';

import {
  dateTime,
  FieldType,
  LoadingState,
  PanelData,
  PanelMenuItem,
  PluginExtensionPanelContext,
  PluginExtensionTypes,
  toDataFrame,
} from '@grafana/data';
import { AngularComponent, getPluginLinkExtensions } from '@grafana/runtime';
import config from 'app/core/config';
import * as actions from 'app/features/explore/state/main';
import { setStore } from 'app/store/store';

import { PanelModel } from '../state';
import { createDashboardModelFixture } from '../state/__fixtures__/dashboardFixtures';

import { getPanelMenu } from './getPanelMenu';

jest.mock('app/core/services/context_srv', () => ({
  contextSrv: {
    hasAccessToExplore: () => true,
  },
}));

jest.mock('@grafana/runtime', () => ({
  ...jest.requireActual('@grafana/runtime'),
  setPluginExtensionGetter: jest.fn(),
  getPluginLinkExtensions: jest.fn(),
}));

const getPluginLinkExtensionsMock = jest.mocked(getPluginLinkExtensions);

describe('getPanelMenu()', () => {
  beforeEach(() => {
    getPluginLinkExtensionsMock.mockRestore();
    getPluginLinkExtensionsMock.mockReturnValue({ extensions: [] });
  });

  it('should return the correct panel menu items', () => {
    const panel = new PanelModel({});
    const dashboard = createDashboardModelFixture({});

    const menuItems = getPanelMenu(dashboard, panel);
    expect(menuItems).toMatchInlineSnapshot(`
      [
        {
          "iconClassName": "eye",
          "onClick": [Function],
          "shortcut": "v",
          "text": "View",
        },
        {
          "iconClassName": "edit",
          "onClick": [Function],
          "shortcut": "e",
          "text": "Edit",
        },
        {
          "iconClassName": "share-alt",
          "onClick": [Function],
          "shortcut": "p s",
          "text": "Share",
        },
        {
          "iconClassName": "compass",
          "onClick": [Function],
          "shortcut": "p x",
          "text": "Explore",
        },
        {
          "iconClassName": "info-circle",
          "onClick": [Function],
          "shortcut": "i",
          "subMenu": [
            {
              "onClick": [Function],
              "text": "Panel JSON",
            },
          ],
          "text": "Inspect",
          "type": "submenu",
        },
        {
          "iconClassName": "cube",
          "onClick": [Function],
          "subMenu": [
            {
              "onClick": [Function],
              "shortcut": "p d",
              "text": "Duplicate",
            },
            {
              "onClick": [Function],
              "text": "Copy",
            },
            {
              "onClick": [Function],
              "text": "Create library panel",
            },
          ],
          "text": "More...",
          "type": "submenu",
        },
        {
          "text": "",
          "type": "divider",
        },
        {
          "iconClassName": "trash-alt",
          "onClick": [Function],
          "shortcut": "p r",
          "text": "Remove",
        },
      ]
    `);
  });

  describe('when extending panel menu from plugins', () => {
    it('should contain menu item from link extension', () => {
      getPluginLinkExtensionsMock.mockReturnValue({
        extensions: [
          {
            id: '1',
            pluginId: '...',
            type: PluginExtensionTypes.link,
            title: 'Declare incident',
            description: 'Declaring an incident in the app',
            path: '/a/grafana-basic-app/declare-incident',
          },
        ],
      });

      const panel = new PanelModel({});
      const dashboard = createDashboardModelFixture({});
      const menuItems = getPanelMenu(dashboard, panel);
      const extensionsSubMenu = menuItems.find((i) => i.text === 'Extensions')?.subMenu;

      expect(extensionsSubMenu).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            text: 'Declare incident',
            href: '/a/grafana-basic-app/declare-incident',
          }),
        ])
      );
    });

    it('should truncate menu item title to 25 chars', () => {
      getPluginLinkExtensionsMock.mockReturnValue({
        extensions: [
          {
            id: '1',
            pluginId: '...',
            type: PluginExtensionTypes.link,
            title: 'Declare incident when pressing this amazing menu item',
            description: 'Declaring an incident in the app',
            path: '/a/grafana-basic-app/declare-incident',
          },
        ],
      });

      const panel = new PanelModel({});
      const dashboard = createDashboardModelFixture({});
      const menuItems = getPanelMenu(dashboard, panel);
      const extensionsSubMenu = menuItems.find((i) => i.text === 'Extensions')?.subMenu;

      expect(extensionsSubMenu).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            text: 'Declare incident when...',
            href: '/a/grafana-basic-app/declare-incident',
          }),
        ])
      );
    });

    it('should pass onClick from plugin extension link to menu item', () => {
      const expectedOnClick = jest.fn();

      getPluginLinkExtensionsMock.mockReturnValue({
        extensions: [
          {
            id: '1',
            pluginId: '...',
            type: PluginExtensionTypes.link,
            title: 'Declare incident when pressing this amazing menu item',
            description: 'Declaring an incident in the app',
            onClick: expectedOnClick,
          },
        ],
      });

      const panel = new PanelModel({});
      const dashboard = createDashboardModelFixture({});
      const menuItems = getPanelMenu(dashboard, panel);
      const extensionsSubMenu = menuItems.find((i) => i.text === 'Extensions')?.subMenu;
      const menuItem = extensionsSubMenu?.find((i) => (i.text = 'Declare incident when...'));

      menuItem?.onClick?.({} as React.MouseEvent);
      expect(expectedOnClick).toBeCalledTimes(1);
    });

    it('should pass context with correct values when configuring extension', () => {
      const data: PanelData = {
        series: [
          toDataFrame({
            fields: [
              { name: 'time', type: FieldType.time },
              { name: 'score', type: FieldType.number },
            ],
          }),
        ],
        timeRange: {
          from: dateTime(),
          to: dateTime(),
          raw: {
            from: 'now',
            to: 'now-1h',
          },
        },
        state: LoadingState.Done,
      };

      const panel = new PanelModel({
        type: 'timeseries',
        id: 1,
        title: 'My panel',
        targets: [
          {
            refId: 'A',
            datasource: {
              type: 'testdata',
            },
          },
        ],
        scopedVars: {
          a: {
            text: 'a',
            value: 'a',
          },
        },
        queryRunner: {
          getLastResult: jest.fn(() => data),
        },
      });

      const dashboard = createDashboardModelFixture({
        timezone: 'utc',
        time: {
          from: 'now-5m',
          to: 'now',
        },
        tags: ['database', 'panel'],
        uid: '123',
        title: 'My dashboard',
      });

      getPanelMenu(dashboard, panel);

      const context: PluginExtensionPanelContext = {
        pluginId: 'timeseries',
        id: 1,
        title: 'My panel',
        timeZone: 'utc',
        timeRange: {
          from: 'now-5m',
          to: 'now',
        },
        targets: [
          {
            refId: 'A',
            datasource: {
              type: 'testdata',
            },
          },
        ],
        dashboard: {
          tags: ['database', 'panel'],
          uid: '123',
          title: 'My dashboard',
        },
        scopedVars: {
          a: {
            text: 'a',
            value: 'a',
          },
        },
        data,
      };

      expect(getPluginLinkExtensionsMock).toBeCalledWith(expect.objectContaining({ context }));
    });

    it('should contain menu item with category', () => {
      getPluginLinkExtensionsMock.mockReturnValue({
        extensions: [
          {
            id: '1',
            pluginId: '...',
            type: PluginExtensionTypes.link,
            title: 'Declare incident',
            description: 'Declaring an incident in the app',
            path: '/a/grafana-basic-app/declare-incident',
            category: 'Incident',
          },
        ],
      });

      const panel = new PanelModel({});
      const dashboard = createDashboardModelFixture({});
      const menuItems = getPanelMenu(dashboard, panel);
      const extensionsSubMenu = menuItems.find((i) => i.text === 'Extensions')?.subMenu;

      expect(extensionsSubMenu).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            text: 'Incident',
            subMenu: expect.arrayContaining([
              expect.objectContaining({
                text: 'Declare incident',
                href: '/a/grafana-basic-app/declare-incident',
              }),
            ]),
          }),
        ])
      );
    });

    it('should truncate category to 25 chars', () => {
      getPluginLinkExtensionsMock.mockReturnValue({
        extensions: [
          {
            id: '1',
            pluginId: '...',
            type: PluginExtensionTypes.link,
            title: 'Declare incident',
            description: 'Declaring an incident in the app',
            path: '/a/grafana-basic-app/declare-incident',
            category: 'Declare incident when pressing this amazing menu item',
          },
        ],
      });

      const panel = new PanelModel({});
      const dashboard = createDashboardModelFixture({});
      const menuItems = getPanelMenu(dashboard, panel);
      const extensionsSubMenu = menuItems.find((i) => i.text === 'Extensions')?.subMenu;

      expect(extensionsSubMenu).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            text: 'Declare incident when...',
            subMenu: expect.arrayContaining([
              expect.objectContaining({
                text: 'Declare incident',
                href: '/a/grafana-basic-app/declare-incident',
              }),
            ]),
          }),
        ])
      );
    });

    it('should contain menu item with category and append items without category after divider', () => {
      getPluginLinkExtensionsMock.mockReturnValue({
        extensions: [
          {
            id: '1',
            pluginId: '...',
            type: PluginExtensionTypes.link,
            title: 'Declare incident',
            description: 'Declaring an incident in the app',
            path: '/a/grafana-basic-app/declare-incident',
            category: 'Incident',
          },
          {
            id: '2',
            pluginId: '...',
            type: PluginExtensionTypes.link,
            title: 'Create forecast',
            description: 'Declaring an incident in the app',
            path: '/a/grafana-basic-app/declare-incident',
          },
        ],
      });

      const panel = new PanelModel({});
      const dashboard = createDashboardModelFixture({});
      const menuItems = getPanelMenu(dashboard, panel);
      const extensionsSubMenu = menuItems.find((i) => i.text === 'Extensions')?.subMenu;

      expect(extensionsSubMenu).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            text: 'Incident',
            subMenu: expect.arrayContaining([
              expect.objectContaining({
                text: 'Declare incident',
                href: '/a/grafana-basic-app/declare-incident',
              }),
            ]),
          }),
          expect.objectContaining({
            type: 'divider',
          }),
          expect.objectContaining({
            text: 'Create forecast',
          }),
        ])
      );
    });
  });

  describe('when panel is in view mode', () => {
    it('should return the correct panel menu items', () => {
      const getExtendedMenu = () => [{ text: 'Toggle legend', shortcut: 'p l', click: jest.fn() }];
      const ctrl = { getExtendedMenu };
      const scope = { $$childHead: { ctrl } };
      const angularComponent = { getScope: () => scope } as AngularComponent;
      const panel = new PanelModel({ isViewing: true });
      const dashboard = createDashboardModelFixture({});

      const menuItems = getPanelMenu(dashboard, panel, angularComponent);
      expect(menuItems).toMatchInlineSnapshot(`
        [
          {
            "iconClassName": "eye",
            "onClick": [Function],
            "shortcut": "v",
            "text": "View",
          },
          {
            "iconClassName": "edit",
            "onClick": [Function],
            "shortcut": "e",
            "text": "Edit",
          },
          {
            "iconClassName": "share-alt",
            "onClick": [Function],
            "shortcut": "p s",
            "text": "Share",
          },
          {
            "iconClassName": "compass",
            "onClick": [Function],
            "shortcut": "p x",
            "text": "Explore",
          },
          {
            "iconClassName": "info-circle",
            "onClick": [Function],
            "shortcut": "i",
            "subMenu": [
              {
                "onClick": [Function],
                "text": "Panel JSON",
              },
            ],
            "text": "Inspect",
            "type": "submenu",
          },
          {
            "iconClassName": "cube",
            "onClick": [Function],
            "subMenu": [
              {
                "href": undefined,
                "onClick": [Function],
                "shortcut": "p l",
                "text": "Toggle legend",
              },
            ],
            "text": "More...",
            "type": "submenu",
          },
        ]
      `);
    });
  });

  describe('onNavigateToExplore', () => {
    const testSubUrl = '/testSubUrl';
    const testUrl = '/testUrl';
    const windowOpen = jest.fn();
    let event: any;
    let explore: PanelMenuItem;
    let navigateSpy: jest.SpyInstance;

    beforeAll(() => {
      const panel = new PanelModel({});
      const dashboard = createDashboardModelFixture({});
      const menuItems = getPanelMenu(dashboard, panel);
      explore = menuItems.find((item) => item.text === 'Explore') as PanelMenuItem;
      navigateSpy = jest.spyOn(actions, 'navigateToExplore');
      window.open = windowOpen;

      event = {
        ctrlKey: true,
        preventDefault: jest.fn(),
      };

      setStore({ dispatch: jest.fn() } as unknown as Store);
    });

    it('should navigate to url without subUrl', () => {
      explore.onClick!(event);

      const openInNewWindow = navigateSpy.mock.calls[0][1].openInNewWindow;

      openInNewWindow(testUrl);

      expect(windowOpen).toHaveBeenLastCalledWith(testUrl);
    });

    it('should navigate to url with subUrl', () => {
      config.appSubUrl = testSubUrl;
      explore.onClick!(event);

      const openInNewWindow = navigateSpy.mock.calls[0][1].openInNewWindow;

      openInNewWindow(testUrl);

      expect(windowOpen).toHaveBeenLastCalledWith(`${testSubUrl}${testUrl}`);
    });
  });
});
