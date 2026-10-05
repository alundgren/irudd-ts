(() => {
  const search = document.querySelector('#recipe-search');
  if (search) {
    const rows = [...document.querySelectorAll('[data-recipe]')];
    const clear = document.querySelector('#clear-search');
    const params = new URLSearchParams(location.search);
    search.value = params.get('q') || '';
    document.querySelector('.search-tools').hidden = false;
    function update(save = true) {
      const terms = search.value.trim().toLowerCase().split(/\s+/).filter(Boolean);
      let count = 0;
      for (const row of rows) {
        row.hidden = !terms.every(term => row.dataset.search.includes(term));
        if (!row.hidden) count++;
        const destination = new URL(row.href);
        search.value ? destination.searchParams.set('q', search.value) : destination.searchParams.delete('q');
        row.href = destination.href;
      }
      document.querySelector('#result-count').textContent = terms.length ? `${count} ${count === 1 ? 'example' : 'examples'}` : '';
      document.querySelector('#empty-results').hidden = count > 0;
      clear.hidden = search.value.length === 0;
      if (save) {
        const url = new URL(location.href);
        search.value ? url.searchParams.set('q', search.value) : url.searchParams.delete('q');
        history.replaceState(null, '', url);
      }
    }
    function reset() {
      search.value = '';
      update();
      search.focus();
    }
    search.addEventListener('input', () => update());
    clear.addEventListener('click', reset);
    search.addEventListener('keydown', event => { if (event.key === 'Escape') reset(); });
    document.querySelector('#reset-search').addEventListener('click', reset);
    window.addEventListener('popstate', () => {
      search.value = new URLSearchParams(location.search).get('q') || '';
      update(false);
    });
    update(false);
    if (params.has('q')) document.querySelector('#examples').scrollIntoView();
  }
  const back = document.querySelector('[data-back-to-results]');
  if (back) {
    const query = new URLSearchParams(location.search).get('q');
    if (query) {
      const destination = new URL(back.href);
      destination.searchParams.set('q', query);
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
