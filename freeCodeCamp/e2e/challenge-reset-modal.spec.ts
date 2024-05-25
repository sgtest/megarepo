import { test, expect } from '@playwright/test';

import translations from '../client/i18n/locales/english/translations.json';
import { clearEditor, focusEditor, getEditors } from './utils/editor';

test('should render the modal content correctly', async ({ page }) => {
  await page.goto(
    '/learn/2022/responsive-web-design/learn-html-by-building-a-cat-photo-app/step-3'
  );

  await page.getByRole('button', { name: translations.buttons.reset }).click();

  await expect(
    page.getByRole('dialog', { name: translations.learn.reset })
  ).toBeVisible();

  await expect(
    page.getByRole('button', {
      name: translations.buttons.close
    })
  ).toBeVisible();
  await expect(
    page.getByRole('heading', {
      name: translations.learn.reset
    })
  ).toBeVisible();

  await expect(page.getByText(translations.learn['reset-warn'])).toBeVisible();
  await expect(
    page.getByText(translations.learn['reset-warn-2'])
  ).toBeVisible();

  await expect(
    page.getByRole('button', {
      name: translations.buttons['reset-lesson']
    })
  ).toBeVisible();
});

test('User can reset challenge', async ({ page, isMobile, browserName }) => {
  const initialText = 'CatPhotoApp';
  const initialFrame = page
    .frameLocator('iframe[title="challenge preview"]')
    .getByText(initialText);

  const updatedText = 'Only Dogs';
  const updatedFrame = page
    .frameLocator('iframe[title="challenge preview"]')
    .getByText(updatedText);

  await page.goto(
    '/learn/2022/responsive-web-design/learn-html-by-building-a-cat-photo-app/step-3'
  );

  // Building the preview can take a while
  await expect(initialFrame).toBeVisible({ timeout: 10000 });

  // Modify the text in the editor pane, clearing first, otherwise the existing
  // text will be selected before typing
  await focusEditor({ page, isMobile });
  await clearEditor({ page, browserName });
  await getEditors(page).fill(updatedText);
  await expect(updatedFrame).toBeVisible({ timeout: 10000 });

  // Run the tests so the lower jaw updates (later we confirm that the update
  // are reset)
  await page
    .getByRole('button', {
      // check-code-2 works on all browsers because it does not include Command
      // or Ctrl
      name: translations.buttons['check-code-2']
    })
    .click();

  await expect(
    page.getByText(translations.learn['sorry-keep-trying'])
  ).toBeVisible();

  // Reset the challenge
  await page.getByTestId('lowerJaw-reset-button').click();
  await page
    .getByRole('button', { name: translations.buttons['reset-lesson'] })
    .click();

  // Check it's back to the initial state
  await expect(initialFrame).toBeVisible({ timeout: 10000 });
  await expect(
    page.getByText(translations.learn['sorry-keep-trying'])
  ).not.toBeVisible();
});

test('User can reset classic challenge', async ({ page, isMobile }) => {
  await page.goto(
    '/learn/javascript-algorithms-and-data-structures/basic-javascript/comment-your-javascript-code'
  );

  const challengeSolution = '// This is in-line comment';

  await getEditors(page).fill(challengeSolution);

  const submitButton = page.getByRole('button', {
    name: isMobile ? translations.buttons.run : translations.buttons['run-test']
  });
  await submitButton.click();

  await expect(
    page.locator('.view-lines').getByText(challengeSolution)
  ).toBeVisible();
  await expect(
    page.getByLabel(translations.icons.passed).locator('circle')
  ).toBeVisible();
  await expect(
    page.getByText(translations.learn['tests-completed'])
  ).toBeVisible();

  await page
    .getByRole('button', { name: translations.buttons['reset-lesson'] })
    .click();
  await page
    .getByRole('button', { name: translations.buttons['reset-lesson'] })
    .click();

  await expect(
    page.locator('.view-lines').getByText(challengeSolution)
  ).not.toBeVisible();
  await expect(
    page.getByLabel(translations.icons.passed).locator('circle')
  ).not.toBeVisible();
  await expect(
    page.getByText(translations.learn['tests-completed'])
  ).not.toBeVisible();
  await expect(page.getByText(translations.learn['test-output'])).toBeVisible();
});

test('should close when the user clicks the close button', async ({ page }) => {
  await page.goto(
    '/learn/2022/responsive-web-design/learn-html-by-building-a-cat-photo-app/step-3'
  );

  await page.getByRole('button', { name: translations.buttons.reset }).click();

  await expect(
    page.getByRole('dialog', { name: translations.learn.reset })
  ).toBeVisible();

  await page
    .getByRole('button', {
      name: translations.buttons.close
    })
    .click();

  await expect(
    page.getByRole('dialog', { name: translations.learn.reset })
  ).toBeHidden();
});

test('User can reset on a multi-file project', async ({ page }) => {
  {
    await page.goto(
      '/learn/javascript-algorithms-and-data-structures-v8/build-a-palindrome-checker-project/build-a-palindrome-checker'
    );
    await page
      .getByRole('button', { name: translations.buttons.reset })
      .click();

    await expect(
      page.getByRole('dialog', { name: translations.learn.reset })
    ).toBeVisible();

    await expect(
      page.getByRole('button', {
        name: translations.buttons.close
      })
    ).toBeVisible();
    await expect(
      page.getByRole('heading', {
        name: translations.learn.reset
      })
    ).toBeVisible();

    await expect(
      page.getByText(translations.learn['reset-warn'])
    ).toBeVisible();
    await expect(
      page.getByText(translations.learn['reset-warn-2'])
    ).toBeVisible();

    await expect(
      page.getByRole('button', {
        name: translations.buttons['reset-lesson']
      })
    ).toBeVisible();
  }
});
