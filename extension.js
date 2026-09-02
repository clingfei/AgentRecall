const vscode = require('vscode');
const fs = require('fs');
const path = require('path');
const os = require('os');
const { execFile } = require('child_process');

let nativeBinaryPath = null;

// Find native Rust binary with Windows (.exe) and Unix support
function getNativeBinary(context) {
    if (nativeBinaryPath && fs.existsSync(nativeBinaryPath)) {
        return nativeBinaryPath;
    }

    const isWin = process.platform === 'win32';
    const exeName = isWin ? 'agentrecall.exe' : 'agentrecall';

    const candidatePaths = [
        context && context.extensionPath ? path.join(context.extensionPath, 'bin', exeName) : null,
        path.join(__dirname, 'bin', exeName),
        path.join(__dirname, 'target', 'release', exeName),
        path.join(os.homedir(), '.cargo', 'bin', exeName),
        isWin ? null : path.join('/usr/local/bin', exeName)
    ].filter(Boolean);

    for (const p of candidatePaths) {
        if (fs.existsSync(p)) {
            nativeBinaryPath = p;
            return p;
        }
    }

    return null;
}

// Run native command helper
function runNative(context, args) {
    const bin = getNativeBinary(context);
    if (!bin) {
        return Promise.reject(new Error('Native binary agentrecall not found'));
    }

    return new Promise((resolve, reject) => {
        execFile(bin, args, { maxBuffer: 100 * 1024 * 1024 }, (err, stdout, stderr) => {
            if (err) {
                reject(new Error(stderr || err.message));
            } else {
                resolve(stdout);
            }
        });
    });
}

// Fallback JS parser if binary is ever missing
const CODEX_DIR = path.join(os.homedir(), '.codex');
const SESSIONS_DIR = path.join(CODEX_DIR, 'sessions');
const INDEX_FILE = path.join(CODEX_DIR, 'session_index.jsonl');

function loadSessionIndex() {
    const map = new Map();
    if (fs.existsSync(INDEX_FILE)) {
        try {
            const content = fs.readFileSync(INDEX_FILE, 'utf-8');
            for (const line of content.split('\n')) {
                if (!line.trim()) continue;
                try {
                    const data = JSON.parse(line);
                    if (data.id) map.set(data.id, data.thread_name || 'Untitled Session');
                } catch (e) {}
            }
        } catch (e) {}
    }
    return map;
}

function getAllSessionFiles(dir = SESSIONS_DIR) {
    let results = [];
    if (!fs.existsSync(dir)) return results;
    try {
        const list = fs.readdirSync(dir, { withFileTypes: true });
        for (const item of list) {
            const fullPath = path.join(dir, item.name);
            if (item.isDirectory()) {
                results = results.concat(getAllSessionFiles(fullPath));
            } else if (item.isFile() && item.name.endsWith('.jsonl')) {
                results.push(fullPath);
            }
        }
    } catch (e) {}
    return results;
}

function getSessionsListFallback() {
    const indexMap = loadSessionIndex();
    const files = getAllSessionFiles();
    const sessions = [];
    const seenIds = new Set();

    for (const fpath of files) {
        let sessionId = null;
        let mtime = 0;
        try {
            const stats = fs.statSync(fpath);
            mtime = stats.mtimeMs;
            const match = fpath.match(/([0-9a-fA-F-]{36})\.jsonl$/);
            if (match) sessionId = match[1];
        } catch (e) {}

        if (sessionId && !seenIds.has(sessionId)) {
            seenIds.add(sessionId);
            sessions.push({
                id: sessionId,
                agent_type: 'OpenAI Codex',
                thread_name: indexMap.get(sessionId) || `Session ${sessionId.slice(0, 8)}`,
                updated_at: new Date(mtime).toISOString(),
                file_path: fpath,
                timestamp_ms: mtime
            });
        }
    }
    sessions.sort((a, b) => b.timestamp_ms - a.timestamp_ms);
    return sessions;
}

// TreeDataProvider for Sidebar & Explorer
class AgentRecallTreeProvider {
    constructor(context) {
        this.context = context;
        this._onDidChangeTreeData = new vscode.EventEmitter();
        this.onDidChangeTreeData = this._onDidChangeTreeData.event;
    }

    refresh() {
        this._onDidChangeTreeData.fire();
    }

    getTreeItem(element) {
        const treeItem = new vscode.TreeItem(element.thread_name, vscode.TreeItemCollapsibleState.None);
        const dateStr = element.updated_at ? new Date(element.updated_at).toLocaleString() : '';
        treeItem.description = dateStr;
        treeItem.tooltip = `ID: ${element.id}\nAgent: ${element.agent_type}\nUpdated: ${dateStr}\nFile: ${element.file_path}`;
        treeItem.iconPath = new vscode.ThemeIcon('comment-discussion');
        treeItem.command = {
            command: 'vscode.open',
            title: 'Open Agent Chat',
            arguments: [vscode.Uri.parse(`agentrecall:/chat/${element.id}.md`)]
        };
        return treeItem;
    }

    async getChildren(element) {
        if (!element) {
            try {
                const stdout = await runNative(this.context, ['list', '--json']);
                return JSON.parse(stdout);
            } catch (e) {
                console.warn('Native binary list failed, using fallback:', e.message);
                return getSessionsListFallback();
            }
        }
        return [];
    }
}

// TextDocumentContentProvider for agentrecall scheme
class AgentRecallContentProvider {
    constructor(context) {
        this.context = context;
    }

    async provideTextDocumentContent(uri) {
        const sessionId = path.basename(uri.path, '.md');
        try {
            const md = await runNative(this.context, ['get', sessionId]);
            return md;
        } catch (e) {
            return `# Error\nUnable to retrieve session ID: \`${sessionId}\`\n\nReason: ${e.message}`;
        }
    }
}

// Open document and scroll/jump precisely to matched range
async function openAndRevealMatch(context, sessionId, anchorText, searchQuery) {
    const uri = vscode.Uri.parse(`agentrecall:/chat/${sessionId}.md`);
    const doc = await vscode.workspace.openTextDocument(uri);
    const text = doc.getText();
    const textLower = text.toLowerCase();

    let targetIndex = -1;
    let matchLen = 0;

    if (anchorText) {
        const anchorLower = anchorText.toLowerCase().replace(/\s+/g, ' ');
        targetIndex = textLower.indexOf(anchorLower);
        if (targetIndex !== -1) {
            matchLen = anchorText.length;
        }
    }

    if (targetIndex === -1 && searchQuery) {
        const queryLower = searchQuery.toLowerCase();
        targetIndex = textLower.indexOf(queryLower);
        if (targetIndex !== -1) {
            matchLen = searchQuery.length;
        }
    }

    let range = undefined;
    if (targetIndex !== -1) {
        const startPos = doc.positionAt(targetIndex);
        const endPos = doc.positionAt(targetIndex + matchLen);
        range = new vscode.Range(startPos, endPos);
    }

    const editor = await vscode.window.showTextDocument(doc, {
        preview: true,
        selection: range
    });

    if (range) {
        editor.revealRange(range, vscode.TextEditorRevealType.InCenter);
        editor.selection = new vscode.Selection(range.start, range.end);
    }
}

function activate(context) {
    const treeProvider = new AgentRecallTreeProvider(context);

    // Register the AgentRecall TreeView IDs contributed by package.json.
    const viewIds = [
        'agentRecallHistoryView',
        'agentRecallHistoryExplorerView'
    ];

    for (const vid of viewIds) {
        try {
            context.subscriptions.push(
                vscode.window.registerTreeDataProvider(vid, treeProvider)
            );
        } catch (e) {}
    }

    const contentProvider = new AgentRecallContentProvider(context);
    const schemes = ['agentrecall'];
    for (const scheme of schemes) {
        context.subscriptions.push(
            vscode.workspace.registerTextDocumentContentProvider(scheme, contentProvider)
        );
    }

    // Command: Search
    const searchHandler = async () => {
        const quickPick = vscode.window.createQuickPick();
        quickPick.placeholder = 'AgentRecall ⚡ Accelerated search with native Rust engine (Press Esc to exit)...';
        quickPick.matchOnDescription = true;
        quickPick.matchOnDetail = true;

        try {
            const listStdout = await runNative(context, ['list', '--json']);
            const sessions = JSON.parse(listStdout);
            quickPick.items = sessions.map(s => ({
                label: `$(comment-discussion) ${s.thread_name}`,
                description: new Date(s.updated_at).toLocaleString(),
                detail: `Agent: ${s.agent_type} | Session ID: ${s.id.slice(0, 8)}... (Click to open full dialogue)`,
                sessionId: s.id
            }));
        } catch (e) {}

        let debounceTimer = null;
        quickPick.onDidChangeValue((value) => {
            const keyword = value.trim();
            if (!keyword) return;

            if (debounceTimer) clearTimeout(debounceTimer);
            quickPick.busy = true;

            debounceTimer = setTimeout(async () => {
                try {
                    const searchStdout = await runNative(context, ['search', keyword, '--json']);
                    const matches = JSON.parse(searchStdout);

                    if (matches.length === 0) {
                        quickPick.items = [{
                            label: `$(warning) No prompts, thoughts, or responses found matching "${keyword}"`,
                            description: 'Try another keyword',
                            detail: ''
                        }];
                    } else {
                        quickPick.items = matches.map(m => {
                            let roleIcon = '$(comment)';
                            let roleLabel = 'User';
                            if (m.role === 'thought') {
                                roleIcon = '$(symbol-keyword)';
                                roleLabel = 'Thought';
                            } else if (m.role === 'assistant') {
                                roleIcon = '$(hubot)';
                                roleLabel = 'Response';
                            }

                            return {
                                label: `${roleIcon} [${roleLabel}] ${m.snippet}`,
                                description: `${m.thread_name}`,
                                detail: `[${m.agent_type}] Time: ${new Date(m.updated_at).toLocaleString()} | Session: ${m.thread_name}`,
                                sessionId: m.session_id,
                                anchorText: m.anchor_text,
                                searchQuery: keyword
                            };
                        });
                    }
                } catch (e) {
                    quickPick.items = [{
                        label: `$(error) Search failed`,
                        description: e.message,
                        detail: ''
                    }];
                }
                quickPick.busy = false;
            }, 80);
        });

        quickPick.onDidAccept(async () => {
            const selected = quickPick.selectedItems[0];
            if (selected && selected.sessionId) {
                quickPick.hide();
                await openAndRevealMatch(context, selected.sessionId, selected.anchorText, selected.searchQuery);
            }
        });

        quickPick.show();
    };

    const cmdNames = ['agentrecall.search'];
    for (const cmd of cmdNames) {
        context.subscriptions.push(vscode.commands.registerCommand(cmd, searchHandler));
    }

    // Command: Refresh History
    const refreshHandler = () => {
        treeProvider.refresh();
        vscode.window.showInformationMessage('AgentRecall session history refreshed');
    };
    const refreshCmds = ['agentrecall.refreshHistory'];
    for (const cmd of refreshCmds) {
        context.subscriptions.push(vscode.commands.registerCommand(cmd, refreshHandler));
    }

    // Command: Sync to workspace
    const syncHandler = async () => {
        const workspaceFolders = vscode.workspace.workspaceFolders;
        let targetDir = '';
        if (workspaceFolders && workspaceFolders.length > 0) {
            targetDir = path.join(workspaceFolders[0].uri.fsPath, '.agentrecall-history');
        } else {
            targetDir = path.join(os.homedir(), 'agentrecall_archive');
        }

        try {
            const exportStdout = await runNative(context, ['export', '-o', targetDir, '--json']);
            const res = JSON.parse(exportStdout);
            vscode.window.showInformationMessage(
                `⚡ AgentRecall native engine exported ${res.count} sessions to ${targetDir}!`
            );
        } catch (e) {
            vscode.window.showErrorMessage(`Export failed: ${e.message}`);
        }
    };
    const syncCmds = ['agentrecall.syncToWorkspace'];
    for (const cmd of syncCmds) {
        context.subscriptions.push(vscode.commands.registerCommand(cmd, syncHandler));
    }
}

function deactivate() {}

module.exports = {
    activate,
    deactivate
};
