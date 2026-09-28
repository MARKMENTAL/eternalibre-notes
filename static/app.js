function app() {
  return {
    previewHtml: window.__INITIAL_PREVIEW__ || '',
    previewVisible: true,
    _previewTimeout: null,

    updatePreview(content) {
      if (this._previewTimeout) {
        clearTimeout(this._previewTimeout);
      }
      this._previewTimeout = setTimeout(() => {
        const formData = new URLSearchParams();
        formData.append('content', content);

        fetch('/preview', {
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
      if (event.ctrlKey || event.metaKey) {
        if (event.key.toLowerCase() === 's') {
          event.preventDefault();
          this.saveCurrentNote();
        } else if (event.key.toLowerCase() === 'n') {
          event.preventDefault();
          const newNoteForm = document.querySelector('form[action="/notes"]');
          if (newNoteForm) newNoteForm.submit();
        }
      }
    }
  };
}
