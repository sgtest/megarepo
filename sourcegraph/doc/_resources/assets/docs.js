window.sgdocs = (() => {
  let VERSION_SELECT_BUTTON,
    SEARCH_FORMS,
    CONTENT_NAV,
    BREADCRUMBS,
    BREADCRUMBS_DATA = [],
    MOBILE_NAV_BUTTON

  return {
    init: breadcrumbs => {
      BREADCRUMBS_DATA = breadcrumbs ? breadcrumbs : []
      BREADCRUMBS = document.querySelector('#breadcrumbs')
      BREADCRUMBS_MOBILE = document.querySelector('#breadcrumbs-mobile')

      SEARCH_FORMS = document.querySelectorAll('.search-form')

      VERSION_SELECTOR = document.querySelector('#version-selector')
      VERSION_SELECT_BUTTON = VERSION_SELECTOR.querySelector('#version-selector button')
      VERSION_OPTIONS = VERSION_SELECTOR.querySelector('#version-selector details-menu')

      CONTENT_NAV = document.querySelector('#content-nav')

      MOBILE_NAV_BUTTON = BREADCRUMBS_MOBILE.querySelector('input[type="button"]')

      searchInit()
      versionSelectorInit()
      mobileNavInit()
      navInit()
      breadcrumbsInit()
      setTimeout(schemaLinkCheck, 0) // Browser scrolls straight to element without this
    },
  }

  /**
   * Smoothly scroll to an element
   *
   * @param {*} element Element to scroll to
   * @param {*} elementOffsetTop Optionally reduce vertical scroll distance
   */
  function scrollToElement(element, elementOffsetTop=0) {
    if (!element) {
      return
    }

    document.body.scrollTo({
      top: element.offsetTop - elementOffsetTop,
      left: 0,
      behavior: 'smooth',
    })
  }

  function searchInit() {
    SEARCH_FORMS.forEach(form => {
      form.addEventListener('submit', e => {
        const search = e.srcElement.querySelector('input[name="search"]').value
        e.preventDefault()
        window.location.href =
          'https://www.google.com/search?ie=UTF-8&q=site%3Adocs.sourcegraph.com+' + encodeURIComponent(search)
      })
    })
  }

  function versionSelectorInit() {
    function outsideVersionSelectorListener(event) {
      if (!event.target.closest('#version-selector')) {
        hideMenu()
      }
    }

    function escaped(e) {
      if (e.which === 27) {
        e.preventDefault()
        hideMenu()
      }
    }
    document.addEventListener('keydown', escaped)

    function hideMenu() {
      VERSION_OPTIONS.classList.remove('show')
      document.removeEventListener('click', outsideVersionSelectorListener)
      document.removeEventListener('keydown', escaped)
    }

    VERSION_SELECT_BUTTON.addEventListener('click', e => {
      VERSION_OPTIONS.classList.toggle('show')
      document.addEventListener('click', outsideVersionSelectorListener)
      document.addEventListener('keydown', escaped)
    })
  }

  function mobileNavInit() {
    MOBILE_NAV_BUTTON.addEventListener('click', e => {
      CONTENT_NAV.classList.toggle('mobile-show')
      document.body.classList.toggle('fix-document.body')
      BREADCRUMBS_MOBILE.classList.toggle('fixed')
    })
  }

  function navInit() {
    if (BREADCRUMBS_DATA[1]) {
      document
        .querySelector(`ul.content-nav-section[data-nav-section="${BREADCRUMBS_DATA[1].Label}"]`)
        .classList.toggle('expanded')
      document
        .querySelector(`ul.content-nav-section a[href="${BREADCRUMBS_DATA[BREADCRUMBS_DATA.length - 1].URL}"]`)
        .parentNode.classList.add('selected')
    }

    document.querySelectorAll('button.content-nav-button').forEach(el => {
      el.addEventListener('click', e => e.srcElement.closest('.content-nav-section').classList.toggle('expanded'))
    })

    document.querySelectorAll('.content-nav a').forEach(el => (el.title = el.text.trim()))
  }

  function breadcrumbsInit() {
    document.querySelectorAll('.breadcrumb-links a').forEach((el, index) => {
      if (index > 0) {
        let text = el.text.replace(/_/g, ' ')
        text = text.charAt(0).toUpperCase() + text.slice(1)
        el.text = text
      }
    })
  }

  /**
   * Check the URL to see if navigation to a schema key is desired
   */
  function schemaLinkCheck() {
    const schemaDocSelector = '.json-schema-doc'
    const offsetTop = document.querySelector('.global-navbar').clientHeight + 20
    const schemaDocs = document.querySelectorAll(schemaDocSelector)

    // Find spans that contain a key and swap them for anchors for hover functionality
    schemaDocs.forEach(schemaDoc => {
      schemaDoc.querySelectorAll(`span`).forEach(el => {
        const keyNameMatch = el.innerText.match(/^"(.*)"/)
        const isKey = el.nextSibling && el.nextSibling.textContent.includes(':') ? true : false

        if (!isKey || !keyNameMatch) {
          return
        }

        // Add a named anchor to get the hover functionality we need
        const keyText = keyNameMatch[1]
        const id = keyText.replace('.', '-')
        const anchor = document
          .createRange()
          .createContextualFragment(
            `<a id="${id}" class="schema-doc-key" href="#${id}" rel="nofollow" aria-hidden="true">"${keyText}"</a>`
          ).firstElementChild
        anchor.style.color = el.style.color
        el.replaceWith(anchor)

        // Temporarily change the id of the anchor to prevent the browser
        // from scrolling to the element
        anchor.addEventListener('click', e => {
          let originalId = e.target.id
          e.target.id = `${originalId}-id-miss`
          setTimeout(() => e.target.id = originalId, 1000)
        })
      })
    })

    // If URL hash is set and matches a schema key, scroll to it
    let targetKey = document.querySelector(`${schemaDocSelector} ${window.location.hash}`)
    if (window.location.hash && targetKey) {
      scrollToElement(targetKey, offsetTop)
    }
  }
})()
