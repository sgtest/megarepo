import { expect, test } from '@playwright/test';
import { clearEditor, focusEditor } from './utils/editor';

test.use({ storageState: 'playwright/.auth/certified-user.json' });

test.describe('Progress bar component', () => {
  test('Should appear with the correct content after the user has submitted their code', async ({
    page,
    isMobile,
    browserName
  }) => {
    await page.goto(
      '/learn/2022/responsive-web-design/learn-html-by-building-a-cat-photo-app/step-3'
    );
    // If focusEditor fails, typically it's because the instructions are too
    // large. There's a bug that means `scrollIntoView` does not work in the
    // editor and so we have to pick less verbose challenges until that's fixed.
    await focusEditor({ page, isMobile });
    await clearEditor({ page, browserName });

    await page.keyboard.insertText(
      '<html><body><h1>CatPhotoApp</h1><h2>Cat Photos</h2><p>See more cat photos in our gallery.</p></body></html>'
    );

    await page.getByRole('button', { name: 'Check Your Code' }).click();

    const progressBarContainer = page.getByTestId('progress-bar-container');
    await expect(progressBarContainer).toContainText(
      'Learn HTML by Building a Cat Photo App'
    );
    await expect(progressBarContainer).toContainText('0% complete');
    await page
      .getByRole('button', { name: 'Submit and go to next challenge' })
      .click();
  });

  test('should appear in the completion modal after user has submitted their code', async ({
    page,
    isMobile,
    browserName
  }) => {
    await page.goto(
      '/learn/javascript-algorithms-and-data-structures/basic-javascript/declare-javascript-variables'
    );
    await focusEditor({ page, isMobile, browserName });
    await clearEditor({ page, browserName });

    await page.keyboard.insertText('var myName;');

    await page
      .getByRole('button', { name: 'Run the Tests (Ctrl + Enter)' })
      .click();

    await expect(page.locator('.completion-block-meta')).toContainText(
      '99% complete'
    );

    await page
      .getByRole('button', { name: 'Submit and go to next challenge' })
      .click();
  });
});
