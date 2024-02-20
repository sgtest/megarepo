---
id: 6571c34868e4b3b17d3957fb
title: Вступ до Flexbox. Запитання J
challengeType: 15
dashedName: introduction-flexbox-question-j
---

# --description--

Розглянемо приклад.

<iframe allowfullscreen="true" allowpaymentrequest="true" allowtransparency="true" class="cp_embed_iframe " frameborder="0" height="400" width="100%" name="cp_embed_1" scrolling="no" src="https://codepen.io/TheOdinProjectExamples/embed/MWoyBzR?height=400&amp;default-tab=html%2Cresult&amp;slug-hash=MWoyBzR&amp;editable=true&amp;user=TheOdinProjectExamples&amp;name=cp_embed_1" style="width: 100%; overflow:hidden; display:block;" title="Вставка CodePen" loading="lazy" id="cp_embed_MWoyBzR"></iframe>

Ви вже маєте здогадатись, що відбудеться, якщо додати `flex: 1` до `.item`. Спробуйте, перш ніж рухатися далі!

Якщо додати `flex: 1` до `.item`, то предмети збільшаться, щоб заповнити порожнє місце. Однак ви хочете, щоб вони залишились тієї ж ширини, але розподілились в контейнері по-іншому. Ви можете це зробити!

Видаліть `flex: 1` з `.item` та додайте `justify-content: space-between` до `.container`. Результат повинен мати схожий вигляд:

<img src="https://cdn.statically.io/gh/TheOdinProject/curriculum/495704c6eb6bf33bc927534f231533a82b27b2ac/html_css/v2/foundations/flexbox/imgs/07.png" alt="зображення з трьома блакитними квадратами, розкиданими по всій ширині контейнера" />

`justify-content` вирівнює предмети по **головній осі**. До цієї властивості можна використати декілька значень. Про решту ви дізнаєтесь в завданнях з читання. А зараз спробуйте використати значення `center`, що відцентрує блоки вздовж головної осі.

# --question--

## --assignment--

Перш ніж перейти до наступного уроку, подивіться, що можливо за допомогою властивості `justify-content`. Прочитайте [цю статтю](https://webdoky.org/uk/docs/Web/CSS/justify-content/#syntaksys) та ознайомтесь з різними значеннями властивості `justify-content` на прикладі.

## --text--

Як застосування властивості `justify-content: space-between` до гнучкого контейнера впливає на розташування його предметів?

## --answers--

Вона рівномірно розподіляє простір між предметами, штовхаючи перший та останній предмети до країв.

---

Вона відцентровує всі предмети в межах контейнера.

---

Вона змушує предмети збільшуватись, щоб заповнити доступний простір.

---

Вона вирівнює предмети за лівим краєм, залишаючи порожній простір справа.

## --video-solution--

1
