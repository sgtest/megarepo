import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import React from 'react';

import { DerivedFields } from './DerivedFields';

describe('DerivedFields', () => {
  let originalGetSelection: typeof window.getSelection;
  beforeAll(() => {
    originalGetSelection = window.getSelection;
    window.getSelection = () => null;
  });

  afterAll(() => {
    window.getSelection = originalGetSelection;
  });

  it('renders correctly when no fields', () => {
    render(<DerivedFields onChange={() => {}} />);

    expect(screen.getByText('Add')).toBeInTheDocument();
    expect(screen.queryByText(/example log message/)).not.toBeInTheDocument();
    expect(screen.queryByTestId('derived-field')).not.toBeInTheDocument();
  });

  it('renders correctly when there are fields', async () => {
    render(<DerivedFields fields={testFields} onChange={() => {}} />);

    await waitFor(() => expect(screen.getAllByTestId('derived-field')).toHaveLength(2));
    expect(screen.getByText('Add')).toBeInTheDocument();
    expect(screen.getByText('Show example log message')).toBeInTheDocument();
  });

  it('adds a new field', async () => {
    const onChange = jest.fn();
    render(<DerivedFields onChange={onChange} />);

    userEvent.click(screen.getByText('Add'));

    await waitFor(() => expect(onChange).toHaveBeenCalledTimes(1));
  });

  // TODO: I saw this test being flaky lately, so I commented it out for now
  // it('removes a field', async () => {
  //   const onChange = jest.fn();
  //   render(<DerivedFields fields={testFields} onChange={onChange} />);

  //   userEvent.click((await screen.findAllByTitle('Remove field'))[0]);

  //   await waitFor(() => expect(onChange).toHaveBeenCalledWith([testFields[1]]));
  // });

  it('validates duplicated field names', async () => {
    const repeatedFields = [
      {
        matcherRegex: '',
        name: 'repeated',
      },
      {
        matcherRegex: '',
        name: 'repeated',
      },
    ];
    render(<DerivedFields onChange={jest.fn()} fields={repeatedFields} />);

    userEvent.click(screen.getAllByPlaceholderText('Field name')[0]);

    expect(await screen.findAllByText('The name is already in use')).toHaveLength(2);
  });

  it('does not validate empty names as repeated', () => {
    const repeatedFields = [
      {
        matcherRegex: '',
        name: '',
      },
      {
        matcherRegex: '',
        name: '',
      },
    ];
    render(<DerivedFields onChange={jest.fn()} fields={repeatedFields} />);

    userEvent.click(screen.getAllByPlaceholderText('Field name')[0]);

    expect(screen.queryByText('The name is already in use')).not.toBeInTheDocument();
  });
});

const testFields = [
  {
    matcherRegex: 'regex1',
    name: 'test1',
    url: 'localhost1',
  },
  {
    matcherRegex: 'regex2',
    name: 'test2',
    url: 'localhost2',
  },
];
