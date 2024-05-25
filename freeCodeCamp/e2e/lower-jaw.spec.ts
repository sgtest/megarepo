import { test, expect } from '@playwright/test';
import { clearEditor, focusEditor, getEditors } from './utils/editor';
import { signout } from './utils/logout';

test.beforeEach(async ({ page }) => {
  await page.goto(
    '/learn/2022/responsive-web-design/learn-html-by-building-a-cat-photo-app/step-3'
  );
});

test('Check the initial states of submit button and "check your code" button', async ({
  page
}) => {
  const checkButton = page.getByTestId('lowerJaw-check-button');

  const submitButton = page.getByTestId('lowerJaw-submit-button');
  const checkButtonState = await checkButton.getAttribute('aria-hidden');
  const submitButtonState = await submitButton.getAttribute('aria-hidden');
  expect(checkButtonState).toBe(null);
  expect(submitButtonState).toBe('true');
});

test('Click on the "check your code" button', async ({ page }) => {
  const checkButton = page.getByRole('button', { name: 'Check Your Code' });

  await checkButton.click();

  const failing = page.getByTestId('lowerJaw-failing-test-feedback');
  const hint = page.getByTestId('lowerJaw-failing-hint');
  await expect(failing).toBeVisible();
  await expect(hint).toBeVisible();
});

test('Resets the lower jaw when prompted', async ({ page }) => {
  const checkButton = page.getByRole('button', { name: 'Check Your Code' });

  await checkButton.click();

  const failing = page.getByTestId('lowerJaw-failing-test-feedback');
  const hint = page.getByTestId('lowerJaw-failing-hint');
  await expect(failing).toBeVisible();
  await expect(hint).toBeVisible();

  await page.getByRole('button', { name: 'Reset' }).click();

  await expect(
    page.getByRole('dialog', { name: 'Reset this lesson?' })
  ).toBeVisible();

  await page.getByRole('button', { name: 'Reset this lesson' }).click();
  await expect(failing).not.toBeVisible();
  await expect(hint).not.toBeVisible();

  await expect(checkButton).toBeVisible();
});

test('Checks hotkeys when instruction is focused', async ({
  page,
  browserName
}) => {
  const editor = getEditors(page);
  const checkButton = page.getByRole('button', { name: 'Check Your Code' });
  const description = page.locator('#description');

  await editor.fill(
    '<h2>Cat Photos</h2>\n<p>See more cat photos in our gallery.</p>'
  );

  await description.click();

  if (browserName === 'webkit') {
    await page.keyboard.press('Meta+Enter');
  } else {
    await page.keyboard.press('Control+Enter');
  }

  await expect(checkButton).not.toBeFocused();
});

test('Focuses on the submit button after tests passed', async ({
  page,
  browserName,
  isMobile
}) => {
  const editor = getEditors(page);
  const checkButton = page.getByRole('button', { name: 'Check Your Code' });
  const submitButton = page.getByRole('button', {
    name: 'Submit and go to next challenge'
  });
  await focusEditor({ page, browserName, isMobile });
  await clearEditor({ page, browserName });

  await editor.fill(
    '<h2>Cat Photos</h2>\n<p>See more cat photos in our gallery.</p>'
  );
  await checkButton.click();

  await expect(submitButton).toBeFocused();
});

test('Prompts unauthenticated user to sign in to save progress', async ({
  page,
  browserName,
  isMobile
}) => {
  await signout(page);
  await page.reload();
  const editor = getEditors(page);
  const checkButton = page.getByRole('button', { name: 'Check Your Code' });
  const loginButton = page.getByRole('link', {
    name: 'Sign in to save your progress'
  });
  await focusEditor({ page, isMobile });
  await clearEditor({ page, browserName });

  await editor.fill(
    '<h2>Cat Photos</h2>\n<p>See more cat photos in our gallery.</p>'
  );

  await checkButton.click();

  await expect(loginButton).toBeVisible();

  await loginButton.click();

  await page.goBack();

  await expect(loginButton).not.toBeVisible();
});

test('Should render UI correctly', async ({ page }) => {
  const codeCheckButton = page.getByRole('button', {
    name: 'Check Your Code'
  });
  const lowerJawTips = page.getByTestId('failing-test-feedback');
  await expect(codeCheckButton).toBeVisible();
  await expect(lowerJawTips).toHaveCount(0);
});

test('Should display the text of the check code button accordingly based on device type and screen size', async ({
  page,
  isMobile,
  browserName
}) => {
  if (isMobile) {
    await expect(
      page.getByRole('button', { name: 'Check Your Code', exact: true })
    ).toBeVisible();
  } else if (browserName === 'webkit') {
    await expect(
      page.getByRole('button', { name: 'Check Your Code (Command + Enter)' })
    ).toBeVisible();
  } else {
    await expect(
      page.getByRole('button', { name: 'Check Your Code (Ctrl + Enter)' })
    ).toBeVisible();
  }
});
