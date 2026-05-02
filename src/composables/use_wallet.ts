import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { save } from '@tauri-apps/plugin-dialog';
import { computed, onMounted, onUnmounted, reactive, ref } from 'vue';
import { toast } from './use_toast';

export type MatchType = 'prefix' | 'suffix' | 'contains';
export type TaskStatus = 'idle' | 'running' | 'limited' | 'stopped' | 'error';

export interface RuleConfig {
    match_types: MatchType[];
    pattern_ids: string[];
    target_count: number;
    max_attempts: number;
    threads?: number;
}

export interface WalletItem {
    id: string;
    address: string;
    private_key: string;
    rule_label: string;
    attempts: number;
    matched: boolean;
    matched_parts: string[];
    matched_rules: string[];
    created_at: string;
}

interface WalletBatch {
    wallets: WalletItem[];
}

export interface ProgressEvent {
    status: TaskStatus;
    attempts: number;
    found_count: number;
    target_count: number;
    speed: number;
    latest_address: string | null;
}

interface GenerateResult {
    wallets: WalletItem[];
    attempts: number;
}

export interface StreamItem {
    id: string;
    address: string;
    matched: boolean;
    matched_parts: string[];
    /**
     * 解码进度：已揭示的字符数，等于 address.length 时解码完成
     */
    decoded: number;
}

export interface PatternOption {
    id: string;
    label: string;
    hint: string;
}

export const match_type_list = [
    { id: 'prefix', label: '前缀', hint: '开头匹配' },
    { id: 'suffix', label: '后缀', hint: '结尾匹配' },
    { id: 'contains', label: '包含', hint: '任意位置' },
] satisfies Array<{ id: MatchType; label: string; hint: string }>;

/**
 * @type {PatternOption[]} 内置靓号规则
 */
export const builtin_patterns: PatternOption[] = [
    { id: 'shape:aaa', label: 'AAA', hint: '三连' },
    { id: 'shape:aaaa', label: 'AAAA', hint: '四连' },
    { id: 'shape:aaaaa', label: 'AAAAA', hint: '五连' },
    { id: 'shape:aaaaaa', label: 'AAAAAA', hint: '六连' },
    { id: 'shape:aabb', label: 'AABB', hint: '双对子' },
    { id: 'shape:aaabbb', label: 'AAABBB', hint: '三三对' },
    { id: 'shape:aabbcc', label: 'AABBCC', hint: '三组对子' },
    { id: 'shape:abba', label: 'ABBA', hint: '镜像' },
    { id: 'shape:abccba', label: 'ABCCBA', hint: '回文' },
    { id: 'shape:abcba', label: 'ABCBA', hint: '回文' },
    { id: 'shape:aabaa', label: 'AABAA', hint: '对称' },
    { id: 'shape:ababab', label: 'ABABAB', hint: '交替' },
    { id: 'shape:abcabc', label: 'ABCABC', hint: '重复组' },
    { id: 'straight:3', label: '三位顺子', hint: '仅数字' },
    { id: 'straight:4', label: '四位顺子', hint: '仅数字' },
    { id: 'straight:5', label: '五位顺子', hint: '仅数字' },
    { id: 'custom:6666', label: '6666', hint: '固定片段' },
    { id: 'custom:8888', label: '8888', hint: '固定片段' },
    { id: 'custom:9999', label: '9999', hint: '固定片段' },
];

const CUSTOM_KEY = 'trx_custom_patterns';

function load_custom_patterns(): PatternOption[] {
    try {
        const raw = localStorage.getItem(CUSTOM_KEY);
        return raw ? JSON.parse(raw) : [];
    } catch {
        return [];
    }
}

function save_custom_patterns(list: PatternOption[]) {
    localStorage.setItem(CUSTOM_KEY, JSON.stringify(list));
}

/**
 * @type {Object} 格式化工具
 */
export const fmt = {
    number(value: number) {
        return new Intl.NumberFormat('zh-CN').format(value);
    },
    date(value: string) {
        return new Intl.DateTimeFormat('zh-CN', {
            dateStyle: 'short',
            timeStyle: 'medium',
        }).format(new Date(value));
    },
    short_key(value: string) {
        if (value.length <= 18) return value;
        return `${value.slice(0, 8)}...${value.slice(-8)}`;
    },
};

export function use_wallet() {
    const rule_config = reactive<RuleConfig>({
        match_types: ['suffix'],
        pattern_ids: ['shape:aaaa'],
        target_count: 0,
        max_attempts: 0,
    });

    const progress = reactive<ProgressEvent>({
        status: 'idle',
        attempts: 0,
        found_count: 0,
        target_count: 0,
        speed: 0,
        latest_address: null,
    });

    const stream_list = ref<StreamItem[]>([]);
    const matched_list = ref<StreamItem[]>([]);
    const history_list = ref<WalletItem[]>([]);
    const history_visible = ref(false);
    const search_text = ref('');
    const matched_only = ref(false);
    const error_text = ref('');
    const is_running = ref(false);
    const is_loading_history = ref(false);
    const custom_input = ref('');
    const thread_count = ref(1);
    const cpu_count = ref(4);

    /**
     * @type {import('vue').Ref<PatternOption[]>} 用户自定义片段列表
     */
    const custom_patterns = ref<PatternOption[]>(load_custom_patterns());

    /**
     * @type {import('vue').ComputedRef<PatternOption[]>} 内置 + 自定义合并列表
     */
    const pattern_list = computed(() => [...builtin_patterns, ...custom_patterns.value]);

    let unlisten_progress: UnlistenFn | null = null;
    let unlisten_batch: UnlistenFn | null = null;

    /**
     * @type {Object} 规则操作
     */
    const rule = {
        toggle_type(id: MatchType) {
            const idx = rule_config.match_types.indexOf(id);
            if (idx >= 0) rule_config.match_types.splice(idx, 1);
            else rule_config.match_types.push(id);
        },
        toggle_pattern(id: string) {
            const idx = rule_config.pattern_ids.indexOf(id);
            if (idx >= 0) rule_config.pattern_ids.splice(idx, 1);
            else rule_config.pattern_ids.push(id);
        },
        select_all() {
            rule_config.pattern_ids = pattern_list.value.map((item) => item.id);
        },
        clear() {
            rule_config.pattern_ids = [];
        },
        status: computed(() => {
            const m: Record<TaskStatus, string> = {
                idle: '等待启动',
                running: '生成中',
                limited: '达到上限',
                stopped: '已停止',
                error: '出错',
            };
            return m[progress.status];
        }),
    };

    /**
     * @type {Object} 自定义片段操作
     */
    const custom = {
        add() {
            const value = custom_input.value.trim();
            if (!value || value.length > 20) return;
            const id = `custom:${value}`;
            if (custom_patterns.value.some((p) => p.id === id)) return;
            custom_patterns.value.push({ id, label: value, hint: '自定义' });
            rule_config.pattern_ids.push(id);
            custom_input.value = '';
            save_custom_patterns(custom_patterns.value);
        },
        remove(id: string) {
            custom_patterns.value = custom_patterns.value.filter((p) => p.id !== id);
            const idx = rule_config.pattern_ids.indexOf(id);
            if (idx >= 0) rule_config.pattern_ids.splice(idx, 1);
            save_custom_patterns(custom_patterns.value);
        },
    };

    /**
     * @type {Object} 任务控制
     */
    const task = {
        async run() {
            if (is_running.value) return;
            error_text.value = '';
            stream_list.value = [];
            matched_list.value = [];
            Object.assign(progress, {
                status: 'running',
                attempts: 0,
                found_count: 0,
                target_count: 0,
                speed: 0,
                latest_address: null,
            });
            is_running.value = true;
            try {
                await invoke<GenerateResult>('generate_wallets', {
                    config: { ...rule_config, threads: thread_count.value },
                });
            } catch (err) {
                progress.status = 'error';
                error_text.value = err instanceof Error ? err.message : String(err);
            } finally {
                is_running.value = false;
            }
        },
        async stop() {
            await invoke('stop_wallets');
        },
    };

    /**
     * @type {Object} 历史记录操作
     */
    const history = {
        async load() {
            is_loading_history.value = true;
            try {
                history_list.value = await invoke<WalletItem[]>('search_wallets', {
                    config: {
                        keyword: search_text.value,
                        limit: 500,
                        matched_only: matched_only.value || null,
                    },
                });
            } finally {
                is_loading_history.value = false;
            }
        },
        async open() {
            history_visible.value = true;
            await history.load();
        },
        close() {
            history_visible.value = false;
        },
        async copy(wallet: WalletItem) {
            const text = `地址: ${wallet.address}\n密钥: ${wallet.private_key}`;
            await navigator.clipboard.writeText(text);
            toast.success('已复制到剪贴板');
        },
        async remove(id: string) {
            await invoke('delete_wallet', { id });
            history_list.value = history_list.value.filter((w) => w.id !== id);
            toast.success('记录已删除');
        },
        async clear_all() {
            await invoke('clear_wallets');
            history_list.value = [];
            toast.success('已清空所有记录');
        },
        async export_all() {
            const path = await save({
                title: '导出钱包记录',
                defaultPath: 'wallets.txt',
                filters: [{ name: 'Text', extensions: ['txt'] }],
            });
            if (!path) return;
            const count = await invoke<number>('export_wallets', { path });
            toast.success(`已导出 ${count} 条记录`);
        },
    };

    /**
     * @type {Object} 地址流操作
     */
    const stream = {
        apply(wallets: WalletItem[]) {
            const now = Date.now();
            for (const w of wallets) {
                const item: StreamItem = {
                    id: `${w.id}-${now}`,
                    address: w.address,
                    matched: w.matched,
                    matched_parts: w.matched_parts,
                    decoded: w.address.length,
                };
                if (w.matched) {
                    matched_list.value = [item, ...matched_list.value].slice(0, 20);
                }
            }
            const sample = wallets.slice(-6).map((w, i) => ({
                id: `${w.id}-${now}-${i}`,
                address: w.address,
                matched: w.matched,
                matched_parts: w.matched_parts,
                decoded: 0,
            }));
            stream_list.value = [...sample, ...stream_list.value].slice(0, 30);
            stream.run_decode();
        },
        push_latest(address: string) {
            if (stream_list.value[0]?.address === address) return;
            const now = Date.now();
            const item: StreamItem = {
                id: `latest-${now}-${address}`,
                address,
                matched: false,
                matched_parts: [],
                decoded: 0,
            };
            stream_list.value = [item, ...stream_list.value].slice(0, 30);
            stream.run_decode();
        },
        /** @private 解码动画帧 ID */
        _raf: 0 as number,
        /** @private 乱码字符池 */
        _glyphs: 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz123456789',
        run_decode() {
            if (stream._raf) return;
            const tick = () => {
                let pending = false;
                const list = stream_list.value;
                for (const item of list) {
                    if (item.decoded < item.address.length) {
                        item.decoded = Math.min(item.decoded + 3, item.address.length);
                        pending = true;
                    }
                }
                if (pending) {
                    stream_list.value = [...list];
                    stream._raf = requestAnimationFrame(tick);
                } else {
                    stream._raf = 0;
                }
            };
            stream._raf = requestAnimationFrame(tick);
        },
        decode_text(item: StreamItem): string {
            if (item.decoded >= item.address.length) return item.address;
            const revealed = item.address.slice(0, item.decoded);
            const remaining = item.address.length - item.decoded;
            let scrambled = '';
            for (let i = 0; i < remaining; i++) {
                scrambled += stream._glyphs[Math.floor(Math.random() * stream._glyphs.length)];
            }
            return revealed + scrambled;
        },
        highlight(address: string, parts: string[]) {
            if (!parts.length) return [{ text: address, hl: false }];
            const sorted = [...parts].sort((a, b) => b.length - a.length);
            const lower = address.toLowerCase();
            const result: Array<{ text: string; hl: boolean }> = [];
            let cursor = 0;
            while (cursor < address.length) {
                let hit = '';
                for (const part of sorted) {
                    if (lower.startsWith(part.toLowerCase(), cursor)) {
                        hit = part;
                        break;
                    }
                }
                if (hit) {
                    result.push({ text: address.slice(cursor, cursor + hit.length), hl: true });
                    cursor += hit.length;
                    continue;
                }
                result.push({ text: address[cursor], hl: false });
                cursor += 1;
            }
            return result.reduce<Array<{ text: string; hl: boolean }>>((list, item) => {
                const prev = list.at(-1);
                if (prev && prev.hl === item.hl) {
                    prev.text += item.text;
                    return list;
                }
                list.push({ ...item });
                return list;
            }, []);
        },
    };

    onMounted(async () => {
        invoke<number>('cpu_count').then((n) => {
            cpu_count.value = n;
            thread_count.value = Math.max(1, Math.floor(n / 2));
        });
        unlisten_progress = await listen<ProgressEvent>('wallet_progress', (event) => {
            Object.assign(progress, event.payload);
            if (event.payload.latest_address) {
                stream.push_latest(event.payload.latest_address);
            }
            if (event.payload.status !== 'running') {
                is_running.value = false;
            }
        });
        unlisten_batch = await listen<WalletBatch>('wallet_batch', (event) => {
            stream.apply(event.payload.wallets);
        });
    });

    onUnmounted(() => {
        unlisten_progress?.();
        unlisten_batch?.();
    });

    return {
        rule_config,
        progress,
        stream_list,
        matched_list,
        history_list,
        history_visible,
        search_text,
        matched_only,
        error_text,
        is_running,
        is_loading_history,
        pattern_list,
        custom_patterns,
        custom_input,
        thread_count,
        cpu_count,
        rule,
        custom,
        task,
        history,
        stream,
    };
}
