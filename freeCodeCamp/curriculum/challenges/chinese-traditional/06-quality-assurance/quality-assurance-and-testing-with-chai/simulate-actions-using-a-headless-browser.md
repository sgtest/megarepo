---
id: 587d824f367417b2b2512c5c
title: 使用無頭瀏覽器模擬操作
challengeType: 2
dashedName: simulate-actions-using-a-headless-browser
---

# --description--

As a reminder, this project is being built upon the following starter project on <a href="https://gitpod.io/?autostart=true#https://github.com/freeCodeCamp/boilerplate-mochachai/" target="_blank" rel="noopener noreferrer nofollow">Gitpod</a>, or cloned from <a href="https://github.com/freeCodeCamp/boilerplate-mochachai/" target="_blank" rel="noopener noreferrer nofollow">GitHub</a>.

在接下來的挑戰中，你將使用無頭瀏覽器模擬人類與頁面的交互。

無頭瀏覽器是沒有 GUI 的 Web 瀏覽器。 它們能夠以與常規瀏覽器相同的方式呈現和解釋 HTML、CSS 和 JavaScript，這使得它們對於測試網頁特別有用。

在下面的挑戰中，你將使用Zombie.js，它是一個輕量級的無頭瀏覽器，不依賴額外的二進制文件來安裝。 但是還有許多其他更強大的無頭瀏覽器選項。

Mocha 允許你在任何實際測試運行之前運行一些代碼。 這對做一些事情很有用，比如向數據庫添加條目，這些條目將在其餘測試中使用。

使用無頭瀏覽器，在運行測試之前，你需要 **訪問** 你要測試的頁面。

`suiteSetup` 鉤子僅在測試套件開始時執行一次。

還有其他幾種鉤子類型，可以在每次測試前、每次測試後或測試套件結束時執行代碼。 有關更多信息，請參閱 Mocha 文檔。

# --instructions--

在 `tests/2_functional-tests.js` 中，緊跟在 `Browser` 聲明之後，將你的項目 URL 添加到變量的 `site` 屬性：

```js
Browser.site = 'http://0.0.0.0:3000'; // Your URL here
```

然後在 `'Functional Tests with Zombie.js'` 套件的根級別，使用以下代碼實例化 `Browser` 對象的新實例：

```js
const browser = new Browser();
```

And use the `suiteSetup` hook to direct the `browser` to the `/` route with the following code. **Note**: `done` is passed as a callback to `browser.visit`, you should not invoke it.

```js
suiteSetup(function(done) {
  return browser.visit('/', done);
});
```

# --hints--

應通過所有測試。

```js
(getUserInput) =>
  $.get(getUserInput('url') + '/_api/get-tests?type=functional&n=4').then(
    (data) => {
      assert.equal(data.state, 'passed');
    },
    (xhr) => {
      throw new Error(xhr.responseText);
    }
  );
```

