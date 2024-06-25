import { render } from '@testing-library/react';
import { useRouteMatch } from 'react-router-dom';

import { useSilenceNavData } from './useSilenceNavData';

jest.mock('react-router-dom', () => ({
  ...jest.requireActual('react-router-dom'),
  useRouteMatch: jest.fn(),
}));

const setup = () => {
  let result: ReturnType<typeof useSilenceNavData>;
  function TestComponent() {
    result = useSilenceNavData();
    return null;
  }

  render(<TestComponent />);

  return { result };
};
describe('useSilenceNavData', () => {
  it('should return correct nav data when route is "/alerting/silence/new"', () => {
    (useRouteMatch as jest.Mock).mockReturnValue({ isExact: true, path: '/alerting/silence/new' });
    const { result } = setup();

    expect(result).toMatchObject({
      text: 'Silence alert rule',
    });
  });

  it('should return correct nav data when route is "/alerting/silence/:id/edit"', () => {
    (useRouteMatch as jest.Mock).mockReturnValue({ isExact: true, path: '/alerting/silence/:id/edit' });
    const { result } = setup();

    expect(result).toMatchObject({
      text: 'Edit silence',
    });
  });
});
