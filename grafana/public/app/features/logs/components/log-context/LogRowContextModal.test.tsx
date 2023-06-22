import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import React from 'react';
import { act } from 'react-dom/test-utils';
import { render } from 'test/redux-rtl';

import { SplitOpenOptions } from '@grafana/data';

import { createLogRow } from '../__mocks__/logRow';

import { LogRowContextModal } from './LogRowContextModal';

const getRowContext = jest.fn().mockResolvedValue({ data: { fields: [], rows: [] } });
const getRowContextQuery = jest.fn().mockResolvedValue({ datasource: { uid: 'test-uid' } });

const dispatchMock = jest.fn();
jest.mock('app/types', () => ({
  ...jest.requireActual('app/types'),
  useDispatch: () => dispatchMock,
}));

const splitOpenSym = Symbol('splitOpen');
const splitOpen = jest.fn().mockReturnValue(splitOpenSym);
jest.mock('app/features/explore/state/main', () => ({
  ...jest.requireActual('app/features/explore/state/main'),
  splitOpen: (arg?: SplitOpenOptions) => {
    return splitOpen(arg);
  },
}));

const row = createLogRow({ uid: '1' });

const timeZone = 'UTC';

describe('LogRowContextModal', () => {
  const originalScrollIntoView = window.HTMLElement.prototype.scrollIntoView;

  beforeEach(() => {
    window.HTMLElement.prototype.scrollIntoView = jest.fn();
  });
  afterEach(() => {
    window.HTMLElement.prototype.scrollIntoView = originalScrollIntoView;
    jest.clearAllMocks();
  });

  it('should not render modal when it is closed', async () => {
    act(() => {
      render(
        <LogRowContextModal
          row={row}
          open={false}
          onClose={() => {}}
          getRowContext={getRowContext}
          timeZone={timeZone}
        />
      );
    });

    await waitFor(() => expect(screen.queryByText('Log context')).not.toBeInTheDocument());
  });

  it('should render modal when it is open', async () => {
    act(() => {
      render(
        <LogRowContextModal
          row={row}
          open={true}
          onClose={() => {}}
          getRowContext={getRowContext}
          timeZone={timeZone}
        />
      );
    });

    await waitFor(() => expect(screen.queryByText('Log context')).toBeInTheDocument());
  });

  it('should call getRowContext on open and change of row', async () => {
    act(() => {
      render(
        <LogRowContextModal
          row={row}
          open={false}
          onClose={() => {}}
          getRowContext={getRowContext}
          timeZone={timeZone}
        />
      );
    });

    await waitFor(() => expect(getRowContext).not.toHaveBeenCalled());
  });
  it('should call getRowContext on open', async () => {
    act(() => {
      render(
        <LogRowContextModal
          row={row}
          open={true}
          onClose={() => {}}
          getRowContext={getRowContext}
          timeZone={timeZone}
        />
      );
    });
    await waitFor(() => expect(getRowContext).toHaveBeenCalledTimes(2));
  });

  it('should call getRowContext when limit changes', async () => {
    act(() => {
      render(
        <LogRowContextModal
          row={row}
          open={true}
          onClose={() => {}}
          getRowContext={getRowContext}
          timeZone={timeZone}
        />
      );
    });
    await waitFor(() => expect(getRowContext).toHaveBeenCalledTimes(2));

    const tenLinesButton = screen.getByRole('button', {
      name: /50 lines/i,
    });
    await userEvent.click(tenLinesButton);
    const twentyLinesButton = screen.getByRole('menuitemradio', {
      name: /20 lines/i,
    });
    act(() => {
      userEvent.click(twentyLinesButton);
    });

    await waitFor(() => expect(getRowContext).toHaveBeenCalledTimes(4));
  });

  it('should call getRowContextQuery when limit changes', async () => {
    render(
      <LogRowContextModal
        row={row}
        open={true}
        onClose={() => {}}
        getRowContext={getRowContext}
        getRowContextQuery={getRowContextQuery}
        timeZone={timeZone}
      />
    );

    // this will call it initially and in the first fetchResults
    await waitFor(() => expect(getRowContextQuery).toHaveBeenCalledTimes(2));

    const tenLinesButton = screen.getByRole('button', {
      name: /50 lines/i,
    });
    await userEvent.click(tenLinesButton);
    const twentyLinesButton = screen.getByRole('menuitemradio', {
      name: /20 lines/i,
    });
    act(() => {
      userEvent.click(twentyLinesButton);
    });

    await waitFor(() => expect(getRowContextQuery).toHaveBeenCalledTimes(3));
  });

  it('should show a split view button', async () => {
    const getRowContextQuery = jest.fn().mockResolvedValue({ datasource: { uid: 'test-uid' } });

    render(
      <LogRowContextModal
        row={row}
        open={true}
        onClose={() => {}}
        getRowContext={getRowContext}
        getRowContextQuery={getRowContextQuery}
        timeZone={timeZone}
      />
    );

    await waitFor(() =>
      expect(
        screen.getByRole('button', {
          name: /open in split view/i,
        })
      ).toBeInTheDocument()
    );
  });

  it('should not show a split view button', async () => {
    render(
      <LogRowContextModal row={row} open={true} onClose={() => {}} getRowContext={getRowContext} timeZone={timeZone} />
    );

    expect(
      screen.queryByRole('button', {
        name: /open in split view/i,
      })
    ).not.toBeInTheDocument();
  });

  it('should call getRowContextQuery', async () => {
    const getRowContextQuery = jest.fn().mockResolvedValue({ datasource: { uid: 'test-uid' } });
    render(
      <LogRowContextModal
        row={row}
        open={true}
        onClose={() => {}}
        getRowContext={getRowContext}
        getRowContextQuery={getRowContextQuery}
        timeZone={timeZone}
      />
    );

    await waitFor(() => expect(getRowContextQuery).toHaveBeenCalledTimes(2));
  });

  it('should close modal', async () => {
    const getRowContextQuery = jest.fn().mockResolvedValue({ datasource: { uid: 'test-uid' } });
    const onClose = jest.fn();
    render(
      <LogRowContextModal
        row={row}
        open={true}
        onClose={onClose}
        getRowContext={getRowContext}
        getRowContextQuery={getRowContextQuery}
        timeZone={timeZone}
      />
    );

    const splitViewButton = await screen.findByRole('button', {
      name: /open in split view/i,
    });

    await userEvent.click(splitViewButton);

    expect(onClose).toHaveBeenCalled();
  });

  it('should create correct splitOpen', async () => {
    const queryObj = { datasource: { uid: 'test-uid' } };
    const getRowContextQuery = jest.fn().mockResolvedValue(queryObj);
    const onClose = jest.fn();

    render(
      <LogRowContextModal
        row={row}
        open={true}
        onClose={onClose}
        getRowContext={getRowContext}
        getRowContextQuery={getRowContextQuery}
        timeZone={timeZone}
      />
    );

    const splitViewButton = await screen.findByRole('button', {
      name: /open in split view/i,
    });

    await userEvent.click(splitViewButton);

    await waitFor(() =>
      expect(splitOpen).toHaveBeenCalledWith(
        expect.objectContaining({
          queries: [queryObj],
          panelsState: {
            logs: {
              id: row.uid,
            },
          },
        })
      )
    );
  });

  it('should dispatch splitOpen', async () => {
    const getRowContextQuery = jest.fn().mockResolvedValue({ datasource: { uid: 'test-uid' } });
    const onClose = jest.fn();

    render(
      <LogRowContextModal
        row={row}
        open={true}
        onClose={onClose}
        getRowContext={getRowContext}
        getRowContextQuery={getRowContextQuery}
        timeZone={timeZone}
      />
    );

    const splitViewButton = await screen.findByRole('button', {
      name: /open in split view/i,
    });

    await userEvent.click(splitViewButton);

    await waitFor(() => expect(dispatchMock).toHaveBeenCalledWith(splitOpenSym));
  });
});
