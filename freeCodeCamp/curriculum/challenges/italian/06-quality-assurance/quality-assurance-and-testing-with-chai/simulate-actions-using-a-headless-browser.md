---
id: 587d824f367417b2b2512c5c
title: Simulare azioni usando un headless browser
challengeType: 2
dashedName: simulate-actions-using-a-headless-browser
---

# --description--

As a reminder, this project is being built upon the following starter project on <a href="https://gitpod.io/?autostart=true#https://github.com/freeCodeCamp/boilerplate-mochachai/" target="_blank" rel="noopener noreferrer nofollow">Gitpod</a>, or cloned from <a href="https://github.com/freeCodeCamp/boilerplate-mochachai/" target="_blank" rel="noopener noreferrer nofollow">GitHub</a>.

Nelle prossime sfide, simulerai l'interazione umana con una pagina utilizzando un headless browser (browser senza testa).

I browser senza intestazione sono browser web senza interfaccia grafica. Essi sono in grado di visualizzare e interpretare HTML, CSS, e JavaScript allo stesso modo in cui farebbe un browser normale, rendendoli particolarmente utili per testare le pagine web.

Per le seguenti sfide userai Zombie.js, che è un browser senza testa leggero che non si basa su binari aggiuntivi da installare. Ma ci sono molte altre, più potenti opzioni di browser senza intestazione.

Mocha consente di eseguire qualche codice prima dell'effettiva esecuzione dei test. Questo può essere utile per fare cose come aggiungere voci a un database che sarà utilizzato nel resto dei test.

Con un browser senza testa, prima di eseguire i test, è necessario **visitare** la pagina da testare.

L'hook `suiteSetup` viene eseguito solo una volta all'inizio di una suite di test.

Ci sono diversi altri tipi di hook che possono eseguire il codice prima di ogni test, dopo ogni test, o alla fine di una suite di test. Vedi la documentazione di Mocha per maggiori informazioni.

# --instructions--

All'interno di `tests/2_functional-tests.js`, immediatamente dopo la dichiarazione `Browser`, aggiungi l'URL del tuo progetto alla proprietà `site` della variabile:

```js
Browser.site = 'http://0.0.0.0:3000'; // Your URL here
```

Poi al livello di root della suite `'Functional Tests with Zombie.js'`, instanzia una nuova istanza dell'oggetto `Browser` con il seguente codice:

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

Tutti i test dovrebbero essere superati.

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

