---
id: 657bdc8ba322aae1eac38390
title: Build a Roman Numeral Converter
challengeType: 14
forumTopicId: 16044
dashedName: build-a-roman-numeral-converter
---

# --description--

Roman numerals are based on seven symbols and can be written using various combinations to represent Arabic numerals. Zum Beispiel:

| Roman numerals | Arabische Ziffern |
| -------------- | ----------------- |
| M              | 1000              |
| CM             | 900               |
| D              | 500               |
| CD             | 400               |
| C              | 100               |
| XC             | 90                |
| L              | 50                |
| XL             | 40                |
| X              | 10                |
| IX             | 9                 |
| V              | 5                 |
| IV             | 4                 |
| I              | 1                 |

**Aufgabe:** Erstelle eine Applikation, die eine ähnliche Funktionalität wie <a href="https://roman-numeral-converter.freecodecamp.rocks" target="_blank" rel="noopener noreferrer nofollow">https://roman-numeral-converter.freecodecamp.rocks</a> aufweist

**User Stories:**

1. You should have an `input` element with an `id` of `number`
1. Du solltest ein `button`-Element mit einer `id` von `convert-btn` haben
1. Du solltest ein `div`-Element mit einer `id` von `output` haben
1. When you click on the `#convert-btn` element without entering a value into the `#number` element, the `#output` element should contain the text `Please enter a valid number`
1. Wenn das `#number`-Element eine Zahl `-1` enthält und das `#convert-btn`-Element angeklickt wird, sollte das `#output`-Element den Text `Please enter a number greater than or equal to 1` enthalten
1. When the `#number` element contains the number `4000` or greater and the `#convert-btn` element is clicked, the `#output` element should contain the text `Please enter a number less than or equal to 3999`
1. Wenn das `#number`-Element eine Zahl `9` enthält und das `#convert-btn`-Element angeklickt wird, sollte das `#output`-Element den Text `IX` enthalten
1. Wenn das `#number`-Element eine Zahl `16` enthält und das `#convert-btn`-Element angeklickt wird, sollte das `#output`-Element den Text `XVI` enthalten
1. Wenn das `#number`-Element eine Zahl `649` enthält und das `#convert-btn`-Element angeklickt wird, sollte das `#output`-Element den Text `DCXLIX` enthalten
1. Wenn das `#number`-Element eine Zahl `1023` enthält und das `#convert-btn`-Element angeklickt wird, sollte das `#output`-Element den Text `MXXIII` enthalten
1. Wenn das `#number`-Element eine Zahl `3999` enthält und das `#convert-btn`-Element angeklickt wird, sollte das `#output`-Element den Text `MMMCMXCIX` enthalten

Erfülle die folgenden User Stories und bestehe alle Tests, um dieses Projekt abzuschließen. Gib dem Ganzen deinen persönlichen Stil. Viel Spaß beim Programmieren!

# --hints--

Du solltest ein `input` Element mit einer `id` von `number` haben.

```js
const el = document.getElementById('number');
assert.strictEqual(el?.nodeName?.toLowerCase(), 'input');
```

Du solltest ein `button`-Element mit einer `id` von `convert-btn` haben.

```js
const el = document.getElementById('convert-btn');
assert.strictEqual(el?.nodeName?.toLowerCase(), 'button');
```

Du solltest ein `div`-Element mit einer `id` von `output` haben.

```js
const el = document.getElementById('output');
assert.strictEqual(el?.nodeName?.toLowerCase(), 'div');
```

When you click on the `#convert-btn` element without entering a value into the `#number` element, the `#output` element should contain the text `Please enter a valid number`.

```js
const numberInputEl = document.getElementById('number');
const convertBtnEl = document.getElementById('convert-btn');
const outputEl = document.getElementById('output');

numberInputEl.value = '';
convertBtnEl.click();
assert.strictEqual(outputEl.innerText.trim().replace(/[.,?!]+$/g, '').toLowerCase(), 'please enter a valid number');
```

Wenn das `#number`-Element eine Zahl `-1` enthält und das `#convert-btn`-Element angeklickt wird, sollte das `#output`-Element den Text `Please enter a number greater than or equal to 1` enthalten

```js
const numberInputEl = document.getElementById('number');
const convertBtnEl = document.getElementById('convert-btn');
const outputEl = document.getElementById('output');

numberInputEl.value = '-1';
convertBtnEl.click();
assert.strictEqual(outputEl.innerText.trim().replace(/[.,?!]+$/g, '').toLowerCase(), 'please enter a number greater than or equal to 1');
```

When the `#number` element contains the number `4000` or greater and the `#convert-btn` element is clicked, the `#output` element should contain the text `Please enter a number less than or equal to 3999`.

```js
const numberInputEl = document.getElementById('number');
const convertBtnEl = document.getElementById('convert-btn');
const outputEl = document.getElementById('output');

numberInputEl.value = '4000';
convertBtnEl.click();
assert.strictEqual(outputEl.innerText.trim().replace(/[.,?!]+$/g, '').toLowerCase(), 'please enter a number less than or equal to 3999');
```

Wenn das `#number`-Element eine Zahl `9` enthält und das `#convert-btn`-Element angeklickt wird, sollte das `#output`-Element den Text `IX` enthalten.

```js
const numberInputEl = document.getElementById('number');
const convertBtnEl = document.getElementById('convert-btn');
const outputEl = document.getElementById('output');

numberInputEl.value = '9';
convertBtnEl.click();
assert.strictEqual(outputEl.innerText.trim(), 'IX');
```

Wenn das `#number`-Element eine Zahl `16` enthält und das `#convert-btn`-Element angeklickt wird, sollte das `#output`-Element den Text `XVI` enthalten.

```js
const numberInputEl = document.getElementById('number');
const convertBtnEl = document.getElementById('convert-btn');
const outputEl = document.getElementById('output');

numberInputEl.value = '16';
convertBtnEl.click();
assert.strictEqual(outputEl.innerText.trim(), 'XVI');
```

Wenn das `#number`-Element eine Zahl `649` enthält und das `#convert-btn`-Element angeklickt wird, sollte das `#output`-Element den Text `DCXLIX` enthalten.

```js
const numberInputEl = document.getElementById('number');
const convertBtnEl = document.getElementById('convert-btn');
const outputEl = document.getElementById('output');

numberInputEl.value = '649';
convertBtnEl.click();
assert.strictEqual(outputEl.innerText.trim(), 'DCXLIX');
```

Wenn das `#number`-Element eine Zahl `1023` enthält und das `#convert-btn`-Element angeklickt wird, sollte das `#output`-Element den Text `MXXIII` enthalten.

```js
const numberInputEl = document.getElementById('number');
const convertBtnEl = document.getElementById('convert-btn');
const outputEl = document.getElementById('output');

numberInputEl.value = '1023';
convertBtnEl.click();
assert.strictEqual(outputEl.innerText.trim(), 'MXXIII');
```

Wenn das `#number`-Element eine Zahl `3999` enthält und das `#convert-btn`-Element angeklickt wird, sollte das `#output`-Element den Text `MMMCMXCIX` enthalten.

```js
const numberInputEl = document.getElementById('number');
const convertBtnEl = document.getElementById('convert-btn');
const outputEl = document.getElementById('output');

numberInputEl.value = '3999';
convertBtnEl.click();
assert.strictEqual(outputEl.innerText.trim(), 'MMMCMXCIX');
```

# --seed--

## --seed-contents--

```html

```

```css

```

```js

```

# --solutions--

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
    <link
      href="https://fonts.googleapis.com/css2?family=Castoro+Titling&display=swap"
      rel="stylesheet"
    />
    <link rel="stylesheet" type="text/css" href="styles.css" />
    <link
      rel="icon"
      type="image/png"
      href="https://cdn.freecodecamp.org/universal/favicons/favicon.ico"
    />
    <title>Roman Numeral Converter</title>
  </head>
  <body>
    <main>
      <img
        class="freecodecamp-logo"
        src="https://cdn.freecodecamp.org/platform/universal/fcc_primary.svg"
        alt="freeCodeCamp Logo"
      />
      <h1>Roman Numeral Converter</h1>
      <form id="form" class="form">
        <fieldset>
          <label for="number">Enter a Number:</label><br />
          <input type="number" id="number" required />
          <button type="button" id="convert-btn">Convert</button>
        </fieldset>
      </form>
      <div id="output" class="output hidden"></div>
    </main>
    <script src="script.js"></script>
  </body>
</html>
```

```css
:root {
  --gray-00: #ffffff;
  --gray-05: #f5f6f7;
  --gray-15: #d0d0d5;
  --gray-75: #3b3b4f;
  --gray-85: #1b1b32;
  --gray-90: #0a0a23;
  --blue-50: #198eee;
  --error: #a94442;
  --danger-color: #850000;
  --danger-background: #ffadad;
}

*,
::before,
::after {
  padding: 0;
  margin: 0;
  box-sizing: border-box;
}

body {
  min-height: 100vh;
  padding: 50px 20px;
  font-family: 'Lato', Helvetica, Arial, sans-serif;
  font-size: 18px;
  background-color: var(--gray-85);
  color: var(--gray-05);
}

main {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
}

.freecodecamp-logo {
  height: 30px;
  margin-bottom: 20px;
}

h1 {
  text-align: center;
  margin: 20px auto;
  max-width: 350px;
  font-family: 'Castoro Titling', cursive;
}

form {
  color: var(--gray-05);
  margin: auto 25px;
  padding: 15px auto;
  border: 3px solid var(--gray-05);
  text-align: center;
  width: 90%;
  max-width: 500px;
  background-color: var(--gray-75);
}

fieldset {
  border: 0 none;
  height: 100%;
  padding: 25px;
  margin: 10px 20px;
}

label {
  display: inline-block;
  font-size: 1.5rem;
  margin-bottom: 10px;
  font-weight: bold;
}

input:focus-visible,
button:focus-visible {
  outline: 3px solid var(--blue-50);
}

input {
  display: block;
  font-size: 2.5rem;
  width: 100%;
  height: 60px;
  padding: 6px 12px;
  margin: 10px 0;
  line-height: 1.4;
  color: white;
  background-color: var(--gray-90);
  border: 1px solid var(--gray-05);
}

button {
  cursor: pointer;
  margin-top: 15px;
  text-decoration: none;
  background-image: linear-gradient(#fecc4c, #ffac33);
  border: 3px solid #feac32;
  padding: 10px 16px;
  font-size: 23px;
  width: 100%;
}

.output {
  color: white;
  background-color: var(--gray-75);
  border: 3px solid var(--gray-05);
  font-size: 2.5rem;
  width: 90%;
  max-width: 500px;
  min-height: 55px;
  margin-top: 25px;
  padding: 15px;
  overflow-wrap: break-word;
  text-align: center;
}

.alert {
  font-size: 2rem;
  background-color: var(--danger-background);
  border: 3px solid var(--danger-color);
  color: var(--danger-color);
}

.hidden {
  display: none;
}
```

```js
const form = document.getElementById("form");
const convertButton = document.getElementById("convert-btn");
const output = document.getElementById("output");

const convertToRoman = (num) => {
  const ref = [
    ["M", 1000],
    ["CM", 900],
    ["D", 500],
    ["CD", 400],
    ["C", 100],
    ["XC", 90],
    ["L", 50],
    ["XL", 40],
    ["X", 10],
    ["IX", 9],
    ["V", 5],
    ["IV", 4],
    ["I", 1],
  ];
  const res = [];

  ref.forEach(function (arr) {
    while (num >= arr[1]) {
      res.push(arr[0]);
      num -= arr[1];
    }
  });

  return res.join("");
};

const isValid = (str, int) => {
  let errText = "";

  if (!str || str.match(/[e.]/g)) {
    errText = "Please enter a valid number.";
  } else if (int < 1) {
    errText = "Please enter a number greater than or equal to 1.";
  } else if (int > 3999) {
    errText = "Please enter a number less than or equal to 3999.";
  } else {
    // No errors detected
    return true;
  }

  // Handle error text and output styling
  output.innerText = errText;
  output.classList.add("alert");

  return false;
};

const clearOutput = () => {
  output.innerText = "";
  output.classList.remove("alert");
};

form.addEventListener("submit", (e) => {
  e.preventDefault();
  updateUI();
});

convertButton.addEventListener("click", () => {
  updateUI();
});

const updateUI = () => {
  const numStr = document.getElementById("number").value;
  const int = parseInt(numStr, 10);

  output.classList.remove("hidden");

  clearOutput();

  if (isValid(numStr, int)) {
    output.innerText = convertToRoman(int);
  }
};
```
