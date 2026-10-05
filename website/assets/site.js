(() => {
  const search = document.querySelector('#recipe-search');
  if (search) {
    const rows = [...document.querySelectorAll('[data-recipe]')];
    const groups = [...document.querySelectorAll('[data-group]')];
    const filters = [...document.querySelectorAll('.filters button')];
    const clear = document.querySelector('#clear-search');
    const params = new URLSearchParams(location.search);
    let category = filters.some(button => button.dataset.category === params.get('category')) ? params.get('category') : 'all';
    search.value = params.get('q') || '';
    document.querySelector('.search-tools').hidden = false;
    function update(save = true) {
      const terms = search.value.trim().toLowerCase().split(/\s+/).filter(Boolean);
      let count = 0;
      for (const row of rows) {
        row.hidden = !(category === 'all' || row.dataset.category === category) || !terms.every(term => row.dataset.search.includes(term));
        if (!row.hidden) count++;
      }
      for (const group of groups) group.hidden = ![...group.querySelectorAll('[data-recipe]')].some(row => !row.hidden);
      for (const button of filters) button.setAttribute('aria-pressed', String(button.dataset.category === category));
      document.querySelector('#result-count').textContent = `${count} ${count === 1 ? 'recipe' : 'recipes'}`;
      document.querySelector('#empty-results').hidden = count > 0;
      clear.hidden = search.value.length === 0;
      for (const row of rows) {
        const destination = new URL(row.href);
        search.value ? destination.searchParams.set('q', search.value) : destination.searchParams.delete('q');
        destination.searchParams.set('category', category);
        row.href = destination.href;
      }
      if (save) {
        const url = new URL(location.href);
        search.value ? url.searchParams.set('q', search.value) : url.searchParams.delete('q');
        category === 'all' ? url.searchParams.delete('category') : url.searchParams.set('category', category);
        history.replaceState(null, '', url);
      }
    }
    function reset() {
      search.value = '';
      category = 'all';
      update();
      search.focus();
    }
    search.addEventListener('input', () => update());
    clear.addEventListener('click', () => { search.value = ''; update(); search.focus(); });
    search.addEventListener('keydown', event => { if (event.key === 'Escape') { search.value = ''; update(); } });
    document.querySelector('#reset-search').addEventListener('click', reset);
    for (const button of filters) button.addEventListener('click', () => { category = button.dataset.category; update(); });
    window.addEventListener('popstate', () => {
      const current = new URLSearchParams(location.search);
      search.value = current.get('q') || '';
      category = filters.some(button => button.dataset.category === current.get('category')) ? current.get('category') : 'all';
      update(false);
    });
    update(false);
    if (params.has('q') || params.has('category')) document.querySelector('#recipes').scrollIntoView();
  }
  const back = document.querySelector('[data-back-to-results]');
  if (back) {
    const params = new URLSearchParams(location.search);
    if (params.has('q') || params.has('category')) {
      const destination = new URL(back.href);
      if (params.has('q')) destination.searchParams.set('q', params.get('q'));
      if (params.has('category')) destination.searchParams.set('category', params.get('category'));
      back.href = destination.href;
      back.textContent = '← Back to results';
    }
  }
  for (const button of document.querySelectorAll('[data-copy]')) {
    button.hidden = false;
    button.addEventListener('click', async () => {
      const code = document.getElementById(button.dataset.copy);
      try {
        await navigator.clipboard.writeText(code.textContent);
        button.textContent = 'Copied';
        button.removeAttribute('data-error');
        document.querySelector('#copy-status').textContent = 'Code copied to clipboard.';
        setTimeout(() => { button.textContent = 'Copy'; }, 1600);
      } catch {
        const selection = window.getSelection();
        const range = document.createRange();
        range.selectNodeContents(code);
        selection.removeAllRanges();
        selection.addRange(range);
        button.textContent = 'Select + copy';
        button.setAttribute('data-error', '');
        document.querySelector('#copy-status').textContent = 'Clipboard unavailable. Code selected. Use your keyboard copy command.';
      }
    });
  }
})();
