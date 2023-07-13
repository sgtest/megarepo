import { getByLabelText, render as rtlRender, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import React from 'react';
import { TestProvider } from 'test/helpers/TestProvider';

import { selectors } from '@grafana/e2e-selectors';

import { wellFormedTree } from '../fixtures/dashboardsTreeItem.fixture';

import { BrowseView } from './BrowseView';

const [mockTree, { folderA, folderA_folderA, folderA_folderB, folderA_folderB_dashbdB, dashbdD, folderB_empty }] =
  wellFormedTree();

function render(...[ui, options]: Parameters<typeof rtlRender>) {
  rtlRender(<TestProvider>{ui}</TestProvider>, options);
}

jest.mock('app/features/browse-dashboards/api/services', () => {
  const orig = jest.requireActual('app/features/browse-dashboards/api/services');

  return {
    ...orig,
    listFolders(parentUID?: string) {
      const childrenForUID = mockTree
        .filter((v) => v.item.kind === 'folder' && v.item.parentUID === parentUID)
        .map((v) => v.item);

      return Promise.resolve(childrenForUID);
    },

    listDashboards(parentUID?: string) {
      const childrenForUID = mockTree
        .filter((v) => v.item.kind === 'dashboard' && v.item.parentUID === parentUID)
        .map((v) => v.item);

      return Promise.resolve(childrenForUID);
    },
  };
});

describe('browse-dashboards BrowseView', () => {
  const WIDTH = 800;
  const HEIGHT = 600;

  it('expands and collapses a folder', async () => {
    render(<BrowseView canSelect folderUID={undefined} width={WIDTH} height={HEIGHT} />);
    await screen.findByText(folderA.item.title);

    await expandFolder(folderA.item.uid);
    expect(screen.queryByText(folderA_folderA.item.title)).toBeInTheDocument();

    await collapseFolder(folderA.item.uid);
    expect(screen.queryByText(folderA_folderA.item.title)).not.toBeInTheDocument();
  });

  it('checks items when selected', async () => {
    render(<BrowseView canSelect folderUID={undefined} width={WIDTH} height={HEIGHT} />);

    const checkbox = await screen.findByTestId(selectors.pages.BrowseDashbards.table.checkbox(dashbdD.item.uid));
    expect(checkbox).not.toBeChecked();

    await userEvent.click(checkbox);
    expect(checkbox).toBeChecked();
  });

  it('checks all descendants when a folder is selected', async () => {
    render(<BrowseView canSelect folderUID={undefined} width={WIDTH} height={HEIGHT} />);
    await screen.findByText(folderA.item.title);

    // First expand then click folderA
    await expandFolder(folderA.item.uid);
    await clickCheckbox(folderA.item.uid);

    // All the visible items in it should be checked now
    const directChildren = mockTree.filter((v) => v.item.kind !== 'ui' && v.item.parentUID === folderA.item.uid);

    for (const child of directChildren) {
      const childCheckbox = screen.queryByTestId(selectors.pages.BrowseDashbards.table.checkbox(child.item.uid));
      expect(childCheckbox).toBeChecked();
    }
  });

  it('checks descendants loaded after a folder is selected', async () => {
    render(<BrowseView canSelect folderUID={undefined} width={WIDTH} height={HEIGHT} />);
    await screen.findByText(folderA.item.title);

    // First expand then click folderA
    await expandFolder(folderA.item.uid);
    await clickCheckbox(folderA.item.uid);

    // When additional children are loaded (by expanding a folder), those items
    // should also be selected
    await expandFolder(folderA_folderB.item.uid);

    const grandchildren = mockTree.filter((v) => v.item.kind !== 'ui' && v.item.parentUID === folderA_folderB.item.uid);

    for (const child of grandchildren) {
      const childCheckbox = screen.queryByTestId(selectors.pages.BrowseDashbards.table.checkbox(child.item.uid));
      expect(childCheckbox).toBeChecked();
    }
  });

  it('unchecks ancestors when unselecting an item', async () => {
    render(<BrowseView canSelect folderUID={undefined} width={WIDTH} height={HEIGHT} />);
    await screen.findByText(folderA.item.title);

    await expandFolder(folderA.item.uid);
    await expandFolder(folderA_folderB.item.uid);

    await clickCheckbox(folderA.item.uid);
    await clickCheckbox(folderA_folderB_dashbdB.item.uid);

    const itemCheckbox = screen.queryByTestId(
      selectors.pages.BrowseDashbards.table.checkbox(folderA_folderB_dashbdB.item.uid)
    );
    expect(itemCheckbox).not.toBeChecked();

    const parentCheckbox = screen.queryByTestId(
      selectors.pages.BrowseDashbards.table.checkbox(folderA_folderB.item.uid)
    );
    expect(parentCheckbox).not.toBeChecked();

    const grandparentCheckbox = screen.queryByTestId(selectors.pages.BrowseDashbards.table.checkbox(folderA.item.uid));
    expect(grandparentCheckbox).not.toBeChecked();
  });

  it('shows indeterminate checkboxes when a descendant is selected', async () => {
    render(<BrowseView canSelect={true} folderUID={undefined} width={WIDTH} height={HEIGHT} />);
    await screen.findByText(folderA.item.title);

    await expandFolder(folderA.item.uid);
    await expandFolder(folderA_folderB.item.uid);

    await clickCheckbox(folderA_folderB_dashbdB.item.uid);

    const parentCheckbox = screen.queryByTestId(
      selectors.pages.BrowseDashbards.table.checkbox(folderA_folderB.item.uid)
    );
    expect(parentCheckbox).not.toBeChecked();
    expect(parentCheckbox).toBePartiallyChecked();

    const grandparentCheckbox = screen.queryByTestId(selectors.pages.BrowseDashbards.table.checkbox(folderA.item.uid));
    expect(grandparentCheckbox).not.toBeChecked();
    expect(grandparentCheckbox).toBePartiallyChecked();
  });

  describe('when there is no item in the folder', () => {
    it('shows a CTA for creating a dashboard if the user has editor rights', async () => {
      render(<BrowseView canSelect={true} folderUID={folderB_empty.item.uid} width={WIDTH} height={HEIGHT} />);
      expect(await screen.findByText('Create Dashboard')).toBeInTheDocument();
    });

    it('shows a simple message if the user has viewer rights', async () => {
      render(<BrowseView canSelect={false} folderUID={folderB_empty.item.uid} width={WIDTH} height={HEIGHT} />);
      expect(await screen.findByText('This folder is empty')).toBeInTheDocument();
    });
  });
});

async function expandFolder(uid: string) {
  const row = screen.getByTestId(selectors.pages.BrowseDashbards.table.row(uid));
  const expandButton = getByLabelText(row, 'Expand folder');
  await userEvent.click(expandButton);
}

async function collapseFolder(uid: string) {
  const row = screen.getByTestId(selectors.pages.BrowseDashbards.table.row(uid));
  const expandButton = getByLabelText(row, 'Collapse folder');
  await userEvent.click(expandButton);
}

async function clickCheckbox(uid: string) {
  const checkbox = screen.getByTestId(selectors.pages.BrowseDashbards.table.checkbox(uid));
  await userEvent.click(checkbox);
}
