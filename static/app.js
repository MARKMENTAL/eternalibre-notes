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
    openSubmenu: null,
    _previewTimeout: null,
    // Id of the note currently being dragged, or null. These live on the
    // component rather than in module-level variables so they are scoped to the
    // single Alpine instance and torn down with it.
    _dragNoteId: null,
    // The nav item currently highlighted as a drop target, or null. Tracked so
    // the highlight follows the pointer instead of accumulating on every item
    // the cursor sweeps across.
    _dropTarget: null,

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

      this.initNoteDragging();
    },

    /// Wires up dragging a sidebar note onto a folder nav item to re-file it.
    ///
    /// Bound on `document` rather than on the elements themselves: the
    /// listeners then survive any re-render of the sidebar, and one `dragover`
    /// handler can serve every drop target.
    initNoteDragging() {
      this._onDragStart = (event) => {
        const item = event.target.closest('.note-item');
        if (!item) return;

        // Dragging a text selection out of the preview should stay a text
        // drag. Without this check, selecting a snippet with the mouse and
        // dragging it would hijack the gesture into a re-file — surprising,
        // and destructive because the drop would be hard to undo.
        //
        // Two conditions, both load-bearing:
        //
        // `hover: hover` gates the whole check. On a touch screen the move
        // gesture is a *long press*, and the press creates a text selection
        // before the drag begins — so testing "is anything selected" would
        // cancel the gesture that mobile users depend on. This guard was
        // written assuming drag was desktop-only and made the long-press
        // re-file fail whenever the press landed on selectable text.
        //
        // `item.contains(...)` scopes it to the item being dragged. The
        // previous page-wide version also refused the drag whenever text was
        // selected anywhere else, including in the editor, which has nothing
        // to do with dragging a sidebar row.
        if (window.matchMedia && window.matchMedia('(hover: hover)').matches) {
          const selection = window.getSelection();
          if (
            selection &&
            !selection.isCollapsed &&
            selection.anchorNode &&
            item.contains(selection.anchorNode)
          ) {
            return;
          }
        }

        const id = item.dataset.noteId;
        if (!id) return;

        this._dragNoteId = id;
        item.classList.add('dragging');
        event.dataTransfer.effectAllowed = 'move';
        event.dataTransfer.setData('text/plain', id);
      };

      this._onDragOver = (event) => {
        if (!this._dragNoteId) return;
        // Only folder nav items accept a note. Everywhere else the default is
        // left alone, so dropping selected text into the editor still works.
        const target = this.dropFolderAt(event.target);
        if (!target) return;
        // Required, or the drop is rejected and never fires `drop`.
        event.preventDefault();
        event.dataTransfer.dropEffect = 'move';
        if (this._dropTarget !== target) {
          if (this._dropTarget) this._dropTarget.classList.remove('drop-target');
          target.classList.add('drop-target');
          this._dropTarget = target;
        }
      };

      this._onDragLeave = (event) => {
        if (!this._dragNoteId) return;
        const item = this.dropFolderAt(event.target);
        if (item && item === this._dropTarget) {
          item.classList.remove('drop-target');
          this._dropTarget = null;
        }
      };

      this._onDrop = (event) => {
        if (!this._dragNoteId) return;
        const target = this.dropFolderAt(event.target);
        if (!target) return;

        // Required, or the browser navigates to the dropped text as a URL and
        // the app is replaced by a 404 page.
        event.preventDefault();

        const id = this._dragNoteId;
        const folder = target.dataset.dropFolder || '';
        this.endNoteDrag();
        this.moveNote(id, folder);
      };

      this._onDragEnd = () => this.endNoteDrag();

      document.addEventListener('dragstart', this._onDragStart);
      document.addEventListener('dragover', this._onDragOver);
      document.addEventListener('dragleave', this._onDragLeave);
      document.addEventListener('drop', this._onDrop);
      document.addEventListener('dragend', this._onDragEnd);
    },

    destroy() {
      if (this._onDocClick) {
        document.removeEventListener('click', this._onDocClick);
      }
      if (this._onDragStart) {
        document.removeEventListener('dragstart', this._onDragStart);
        document.removeEventListener('dragover', this._onDragOver);
        document.removeEventListener('dragleave', this._onDragLeave);
        document.removeEventListener('drop', this._onDrop);
        document.removeEventListener('dragend', this._onDragEnd);
      }
    },

    /// The folder nav item under `node`, or null if the pointer is elsewhere.
    ///
    /// `closest` rather than `matches`, because the pointer is usually over
    /// the `<a>` or a `<span>` inside the item rather than the `<li>` itself.
    dropFolderAt(node) {
      if (!node || !node.closest) return null;
      return node.closest('[data-drop-folder]');
    },

    /// Clears every trace of an in-flight drag.
    ///
    /// Runs on `drop` and on `dragend`. `dragend` is the one that matters: it
    /// fires when a drag is abandoned (Escape, dropped outside any target), and
    /// without it the dragged item would stay dimmed and the folder highlighted
    /// for the rest of the session.
    endNoteDrag() {
      this._dragNoteId = null;
      if (this._dropTarget) {
        this._dropTarget.classList.remove('drop-target');
        this._dropTarget = null;
      }
      document.querySelectorAll('.note-item.dragging').forEach((el) => {
        el.classList.remove('dragging');
      });
    },

    /// Posts a re-file for `id` and reloads so the sidebar reflects the move.
    moveNote(id, folder) {
      const base = window.__BASE_PATH__ || '';
      const body = new URLSearchParams();
      body.append('folder', folder);

      fetch(base + '/notes/' + encodeURIComponent(id) + '/move', {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: body.toString()
      })
        .then(() => {
          // Reload rather than following the redirect: if the note was just
          // moved out of the folder being viewed, reloading keeps the user in
          // that folder's now-correct listing instead of yanking them to the
          // note page.
          window.location.reload();
        })
        .catch((err) => {
          console.error('Move failed:', err);
          window.location.reload();
        });
    },

    toggleMenu(name) {
      this.openMenu = this.openMenu === name ? null : name;
      this.openSubmenu = null;
    },

    toggleSubmenu(name, event) {
      // Stop this click before it reaches the File menu's trigger, which would
      // otherwise close the whole menu as the submenu opens.
      event.stopPropagation();
      this.openSubmenu = this.openSubmenu === name ? null : name;
    },

    closeMenus() {
      this.openMenu = null;
      this.openSubmenu = null;
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
