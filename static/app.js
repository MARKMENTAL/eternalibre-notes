function app() {
  return {
    previewHtml: window.__INITIAL_PREVIEW__ || '',
    previewVisible: true,
    editorVisible: true,
    tab: window.__DEFAULT_TAB__ || 'notes',
    // Only one menu can be open at a time. Keeping a single name here (rather
    // than a set of booleans) makes mutual exclusion automatic: opening Theme
    // while Help is open reassigns this value, so Help's dropdown is hidden.
    openMenu: null,
    _previewTimeout: null,

    init() {
      // Close menus on any click outside the menu bar. Alpine's `.outside`
      // modifier cannot be used here because Maud treats `.` in attribute
      // names as CSS class shorthand, so we bind it manually.
      this._onDocClick = (event) => {
        const menuBar = document.querySelector('.menu-bar');
        if (this.openMenu && menuBar && !menuBar.contains(event.target)) {
          this.closeMenus();
        }
      };
      document.addEventListener('click', this._onDocClick);
    },

    destroy() {
      if (this._onDocClick) {
        document.removeEventListener('click', this._onDocClick);
      }
    },

    toggleMenu(name) {
      this.openMenu = this.openMenu === name ? null : name;
    },

    closeMenus() {
      this.openMenu = null;
    },

    setTab(name) {
      this.tab = name;
      // Selecting a tab should always reveal its pane, even if that pane had
      // been collapsed from the View menu.
      if (name === 'preview') this.previewVisible = true;
      if (name === 'write') this.editorVisible = true;
    },

    updatePreview(content) {
      if (this._previewTimeout) {
        clearTimeout(this._previewTimeout);
      }
      this._previewTimeout = setTimeout(() => {
        const formData = new URLSearchParams();
        formData.append('content', content);

        // The base path has to be prepended here. `app.js` is inlined into
        // the page, so it cannot call the server-side `base.url()` helper
        // that every form action uses — and an origin-absolute `/preview`
        // resolves against the site root, which behind a reverse proxy
        // mounted at a subdirectory never reaches the app. The response body
        // would then be the proxy's own 404, swapped straight into the
        // preview pane.
        const base = window.__BASE_PATH__ || '';

        fetch(base + '/preview', {
          method: 'POST',
          headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
          body: formData.toString()
        })
          .then(r => r.text())
          .then(html => {
            this.previewHtml = html;
          })
          .catch(err => console.error('Preview update failed:', err));
      }, 300);
    },

    togglePreview() {
      this.previewVisible = !this.previewVisible;
    },

    toggleEditor() {
      this.editorVisible = !this.editorVisible;
    },

    focusEditor() {
      const editor = document.getElementById('editor');
      if (editor) editor.focus();
    },

    saveCurrentNote() {
      const form = document.getElementById('note-form');
      if (form) form.requestSubmit();
    },

    filterNotes(query) {
      const term = query.toLowerCase();
      document.querySelectorAll('.note-item').forEach(item => {
        const title = item.dataset.title || '';
        const content = item.dataset.content || '';
        item.style.display = (title.includes(term) || content.includes(term)) ? '' : 'none';
      });
    },

    handleKeydown(event) {
      if (event.key === 'Escape' && this.openMenu) {
        this.closeMenus();
        return;
      }
      if (event.ctrlKey || event.metaKey) {
        if (event.key.toLowerCase() === 's') {
          event.preventDefault();
          this.saveCurrentNote();
        } else if (event.key.toLowerCase() === 'n') {
          event.preventDefault();
          // Match on the path ending in /notes rather than an exact action,
          // so this still works when the app is mounted under a base path.
          const newNoteForm = document.querySelector(
            'form[action$="/notes"], form[action$="/notes/"]'
          );
          if (newNoteForm) newNoteForm.submit();
        }
      }
    }
  };
}
