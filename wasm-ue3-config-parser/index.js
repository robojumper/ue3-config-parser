const rust = import('./pkg');
import * as monaco from 'monaco-editor';

var editor = undefined;
var decorations = [];

// Check browser support
if (typeof TextEncoder === 'undefined' || typeof TextDecoder === 'undefined' || typeof Uint8Array === 'undefined') {
	var upload = document.getElementById('editor');
	upload.textContent = "Your browser does not support the necessary features. Use Firefox, Chrome, or a new (79+) Edge version."
	throw new Error("Bad browser support");
}

rust.then(wasm => {

	function stopEvent(e) {
		e.stopPropagation();
		e.preventDefault();
	}

	function handleUpload(file) {
		var reader = new FileReader();
		reader.onload = function () {
			var text = this.result;
			editor.setValue(text);
		}
		reader.readAsText(file);
	}

	function onUpload() {
		var upload = document.getElementById('filepick');
		const curFiles = upload.files;

		if (curFiles.length == 1) {
			var file = curFiles[0];
			handleUpload(file);
		} else {
			return;
		}
	}

	function dropHandler(ev) {
		// Thanks MDN
		// Prevent default behavior (Prevent file from being opened)
		stopEvent(ev);
		var file = null;
		if (ev.dataTransfer.items) {
			if (ev.dataTransfer.items.length === 1 && ev.dataTransfer.items[0].kind === 'file') {
				// Use DataTransferItemList interface to access the file
				file = ev.dataTransfer.items[0].getAsFile();
			} else {
				return;
			}
		} else {
			// Use DataTransfer interface to access the file(s)
			if (ev.dataTransfer.files.length === 1) {
				file = ev.dataTransfer.files[0];
			} else {
				return;
			}
		}
		console.log('file.name = ' + file.name);

		handleUpload(file);
	}

	function onChange(ev) {
		var text = editor.getValue();
		let annots = wasm.check(text).annots;
		console.log(annots);
		var markers = [];
		var new_decorations = [];
		for (var annot of annots) {
			markers.push({
				startLineNumber: annot.line, startColumn: annot.col, endLineNumber: annot.eline, endColumn: annot.ecol,
				severity: monaco.MarkerSeverity.Error, message: annot.err
			});

			var range = new monaco.Range(annot.line, annot.col, annot.eline, annot.ecol);
			new_decorations.push({range: range, options: {glyphMarginClassName: "margin-error"}})

		}
		decorations = editor.getModel().deltaDecorations(decorations, new_decorations);
		console.log(markers);
		monaco.editor.setModelMarkers(editor.getModel(), "i", markers);
	}

	wasm.init();

	var upload = document.getElementById('filepick');
	upload.value = "";
	upload.addEventListener('change', onUpload);

	editor = monaco.editor.create(document.getElementById("editor"), {
		value: [
			'[SuperAwesome.ButBuggyHeader] ',
			'+IgnoredEntry=":("',
			'',
			'[Package.CorrectHeader]',
			'+MyArray=(Entry[0]="Abc" \\\\ ',
			')',
			'',
			'+MyArray=(Entry[0]="Abc" \\\\',
			'; Entry[1]="Def", \\\\',
			')',
			'',
			'whateverthisis',
			'',
			'!FailToClear',
			'',
			'[AnotherBuggyHeader] ; this is not a comment',
			'',
			'; comment',
		].join('\n'),
		language: 'ini',
		theme: 'vs-dark',
		glyphMargin: true,
	});

	editor.onDidChangeModelContent(e => onChange());
	editor.getContainerDomNode().addEventListener("dragenter", stopEvent, false);
	editor.getContainerDomNode().addEventListener("dragover", stopEvent, false);
	editor.getContainerDomNode().addEventListener("drop", dropHandler, false);
	onChange();
});