---
id: bd7158d8c443edefaeb5bd0f
title: 文件元数据微服务
challengeType: 4
forumTopicId: 301506
dashedName: file-metadata-microservice
---

# --description--

构建一个功能类似于此的全栈 JavaScript 应用程序：<a href="https://file-metadata-microservice.freecodecamp.rocks" target="_blank" rel="noopener noreferrer nofollow">https://file-metadata-microservice.freecodecamp.rocks</a>。 在这个项目中，你将使用以下方法之一编写你的代码：

-   克隆<a href="https://github.com/freeCodeCamp/boilerplate-project-filemetadata/" target="_blank" rel="noopener noreferrer nofollow">这个 GitHub 仓库</a>，并在本地完成你的项目。
-   Use <a href="https://gitpod.io/?autostart=true#https://github.com/freeCodeCamp/boilerplate-project-filemetadata/" target="_blank" rel="noopener noreferrer nofollow">our Gitpod starter project</a> to complete your project.
-   使用你选择的网站生成器来完成项目。 需要包含我们 GitHub 仓库的所有文件。

# --instructions--

** 提示：**可以使用 `multer` npm 包来处理上传文件。

# --hints--

你应该提交自己的项目，而不是示例的 URL。

```js
(getUserInput) => {
  assert(
    !/.*\/file-metadata-microservice\.freecodecamp\.rocks/.test(
      getUserInput('url')
    )
  );
};
```

你可以提交一个包含上传文件的表单。

```js
async (getUserInput) => {
  const site = await fetch(getUserInput('url'));
  const data = await site.text();
  const doc = new DOMParser().parseFromString(data, 'text/html');
  assert(doc.querySelector('input[type="file"]'));
};
```

表单文件输入字段的 `name` 属性设置成 `upfile`。

```js
async (getUserInput) => {
  const site = await fetch(getUserInput('url'));
  const data = await site.text();
  const doc = new DOMParser().parseFromString(data, 'text/html');
  assert(doc.querySelector('input[name="upfile"]'));
};
```

当你提交一个文件时，在 JSON 响应中收到文件的 `name`、`type` 和 `size`，以 bytes（字节）为单位。

```js
async (getUserInput) => {
  const formData = new FormData();
  const fileData = await fetch(
    'https://cdn.freecodecamp.org/weather-icons/01d.png'
  );
  const file = await fileData.blob();
  formData.append('upfile', file, 'icon');
  const data = await fetch(getUserInput('url') + '/api/fileanalyse', {
    method: 'POST',
    body: formData
  });
  const parsed = await data.json();
  assert.property(parsed, 'size');
  assert.equal(parsed.name, 'icon');
  assert.equal(parsed.type, 'image/png');
};
```

