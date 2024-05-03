import { APIRequestContext, expect } from '@playwright/test';

const ensureLeadingSlash = (endpoint: string) =>
  endpoint[0] === '/' ? endpoint : '/' + endpoint;

export const authedPut = async (
  request: APIRequestContext,
  endpoint: string,
  data: Record<string, unknown>
) => {
  const csrfToken = (await request.storageState()).cookies.find(
    c => c.name === 'csrf_token'
  )?.value;

  expect(csrfToken).toBeTruthy();

  const res = await request.put(
    process.env.API_LOCATION + ensureLeadingSlash(endpoint),
    {
      data,
      headers: { 'csrf-token': csrfToken! }
    }
  );
  expect(res.status()).toBe(200);
  return res;
};
