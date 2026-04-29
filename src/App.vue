<script setup lang="ts">
import { ChevronDown, Clock, Copy, Play, Plus, QrCode, Search, Square, Trash2, X } from 'lucide-vue-next';
import QRCode from 'qrcode';
import { nextTick, ref, watch, type CSSProperties } from 'vue';
import logoSvg from './assets/logo.svg';
import { toast } from './composables/use_toast';
import type { WalletItem } from './composables/use_wallet';
import { builtin_patterns, fmt, match_type_list, use_wallet } from './composables/use_wallet';

const thread_open = ref(false);
const thread_btn = ref<HTMLElement | null>(null);

/**
 * @type {import('vue').Ref<WalletItem | null>} 当前展示二维码的钱包
 */
const qr_wallet = ref<WalletItem | null>(null);

/**
 * @type {import('vue').Ref<'address' | 'private_key'>} 二维码内容类型
 */
const qr_mode = ref<'address' | 'private_key'>('private_key');
const qr_canvas = ref<HTMLCanvasElement | null>(null);

async function render_qr() {
    const wallet = qr_wallet.value;
    const canvas = qr_canvas.value;
    if (!wallet || !canvas) return;
    const text = qr_mode.value === 'address' ? wallet.address : wallet.private_key;
    await QRCode.toCanvas(canvas, text, {
        width: 220,
        margin: 2,
        color: { dark: '#e8e6f0', light: '#00000000' },
        errorCorrectionLevel: 'M',
    });
}

function show_qr(wallet: WalletItem) {
    qr_wallet.value = wallet;
    qr_mode.value = 'private_key';
    nextTick(render_qr);
}

watch(qr_mode, () => nextTick(render_qr));

/**
 * @type {import('vue').Ref<CSSProperties>} 弹窗定位样式
 */
const thread_popup_style = ref<CSSProperties>({});

function update_popup_pos() {
    const el = thread_btn.value;
    if (!el) return;
    const r = el.getBoundingClientRect();
    thread_popup_style.value = {
        position: 'fixed',
        right: `${window.innerWidth - r.right}px`,
        bottom: `${window.innerHeight - r.top + 4}px`,
        minWidth: `${r.width}px`,
    };
}

watch(thread_open, (v) => {
    if (v) {
        update_popup_pos();
        window.addEventListener('resize', update_popup_pos);
        window.addEventListener('scroll', update_popup_pos, true);
    } else {
        window.removeEventListener('resize', update_popup_pos);
        window.removeEventListener('scroll', update_popup_pos, true);
    }
});
const {
    rule_config,
    progress,
    stream_list,
    matched_list,
    history_list,
    history_visible,
    search_text,
    error_text,
    is_running,
    is_loading_history,
    custom_patterns,
    custom_input,
    thread_count,
    cpu_count,
    rule,
    custom,
    task,
    history,
    stream,
} = use_wallet();
</script>

<template>
    <main class="flex flex-col h-screen p-5 gap-4 overflow-hidden">
        <!-- 顶栏 -->
        <header
            class="flex justify-between items-center px-5 py-3 shrink-0 rounded-xl border border-border bg-surface backdrop-blur-xl shadow-[0_1px_24px_oklch(0.68_0.14_250/0.06)]"
        >
            <div class="flex items-center gap-3">
                <img :src="logoSvg" alt="logo" class="w-10 h-10 rounded-lg" />
                <div>
                    <p class="m-0 text-[10px] font-semibold tracking-[0.2em] uppercase text-accent">TRX VANITY STUDIO</p>
                    <h1 class="m-0 mt-0.5 text-lg font-semibold text-text-bright tracking-tight">靓号地址生成器</h1>
                </div>
            </div>
            <button
                class="inline-flex items-center gap-2 h-8 px-3.5 rounded-lg text-xs font-medium text-text-muted border border-border bg-transparent cursor-pointer transition-colors duration-200 hover:text-text-bright hover:border-accent/30"
                @click="history.open"
            >
                <Clock :size="14" />
                历史记录
            </button>
        </header>

        <!-- 主体 -->
        <section class="grid grid-cols-[380px_minmax(0,1fr)] gap-4 flex-1 min-h-0">
            <!-- 左侧面板 -->
            <aside
                class="flex flex-col gap-3 p-4 overflow-y-auto rounded-xl border border-border bg-surface backdrop-blur-xl shadow-[inset_0_1px_0_oklch(1_0_0/0.04)]"
            >
                <!-- 匹配类型 -->
                <div class="p-3 rounded-lg border border-border-dim">
                    <h2 class="m-0 text-xs font-semibold text-text-muted uppercase tracking-wider">匹配类型</h2>
                    <div class="grid grid-cols-3 gap-1.5 mt-2">
                        <button
                            v-for="item in match_type_list"
                            :key="item.id"
                            class="flex flex-col items-center gap-0.5 py-2 rounded-lg text-center border cursor-pointer transition-all duration-200"
                            :class="
                                rule_config.match_types.includes(item.id)
                                    ? 'border-accent/40 bg-accent-dim text-accent'
                                    : 'border-border-dim bg-surface-alt text-text-muted hover:border-accent/20'
                            "
                            @click="rule.toggle_type(item.id)"
                        >
                            <span class="text-[12px] font-semibold">{{ item.label }}</span>
                            <span class="text-[10px] opacity-60">{{ item.hint }}</span>
                        </button>
                    </div>
                </div>

                <!-- 内置规则 -->
                <div class="flex-1 min-h-0 flex flex-col p-3 rounded-lg border border-border-dim">
                    <div class="flex justify-between items-center">
                        <h2 class="m-0 text-xs font-semibold text-text-muted uppercase tracking-wider">内置规则</h2>
                        <div class="flex gap-1">
                            <button
                                class="px-2 py-0.5 text-[10px] text-text-muted bg-transparent border-0 cursor-pointer hover:text-accent transition-colors"
                                @click="rule.select_all"
                            >
                                全选
                            </button>
                            <button
                                class="px-2 py-0.5 text-[10px] text-text-muted bg-transparent border-0 cursor-pointer hover:text-error transition-colors"
                                @click="rule.clear"
                            >
                                清空
                            </button>
                        </div>
                    </div>
                    <div class="grid grid-cols-2 gap-2.5 mt-2 overflow-y-auto flex-1 pr-0.5">
                        <button
                            v-for="item in builtin_patterns"
                            :key="item.id"
                            class="flex items-center gap-2 h-9 px-2.5 rounded-lg border cursor-pointer transition-all duration-200"
                            :class="
                                rule_config.pattern_ids.includes(item.id)
                                    ? 'border-accent/40 bg-accent-dim text-accent'
                                    : 'border-border-dim bg-surface-alt text-text-muted hover:border-accent/20'
                            "
                            @click="rule.toggle_pattern(item.id)"
                        >
                            <span class="text-[12px] font-semibold">{{ item.label }}</span>
                            <span class="text-[10px] opacity-50">{{ item.hint }}</span>
                        </button>
                    </div>
                </div>

                <!-- 自定义规则 -->
                <div class="p-3 rounded-lg border border-border-dim">
                    <h2 class="m-0 text-xs font-semibold text-text-muted uppercase tracking-wider">自定义规则</h2>
                    <div class="flex gap-1.5 mt-2">
                        <input
                            v-model="custom_input"
                            class="flex-1 h-8 px-2.5 rounded-lg text-xs text-text-bright bg-surface-alt border border-border-dim outline-none placeholder:text-text-muted/50 focus:border-accent/40 transition-colors"
                            placeholder="输入规则，如 777、ABC"
                            maxlength="20"
                            @keydown.enter="custom.add"
                        />
                        <button
                            class="flex items-center justify-center w-8 h-8 rounded-lg border border-border-dim bg-surface-alt text-text-muted cursor-pointer hover:text-accent hover:border-accent/40 transition-colors"
                            @click="custom.add"
                        >
                            <Plus :size="14" />
                        </button>
                    </div>
                    <div v-if="custom_patterns.length" class="flex flex-wrap gap-1.5 mt-2 max-h-24 overflow-y-auto">
                        <span
                            v-for="item in custom_patterns"
                            :key="item.id"
                            class="inline-flex items-center gap-1 h-7 px-2 rounded-md text-[11px] font-medium cursor-pointer transition-all duration-200"
                            :class="
                                rule_config.pattern_ids.includes(item.id)
                                    ? 'bg-accent-dim text-accent border border-accent/40'
                                    : 'bg-surface-alt text-text-muted border border-border-dim'
                            "
                            @click="rule.toggle_pattern(item.id)"
                        >
                            {{ item.label }}
                            <X :size="10" class="opacity-50 hover:opacity-100 hover:text-error" @click.stop="custom.remove(item.id)" />
                        </span>
                    </div>
                </div>

                <!-- 运行设置 -->
                <div class="p-3 rounded-lg border border-border-dim">
                    <h2 class="m-0 text-xs font-semibold text-text-muted uppercase tracking-wider">运行设置</h2>
                    <div class="flex items-center gap-2 mt-2">
                        <div class="flex items-center gap-1.5 flex-1 h-9 px-3 rounded-lg border border-border-dim bg-surface-alt">
                            <span class="text-xs text-text-muted whitespace-nowrap">目标数量</span>
                            <input
                                v-model.number="rule_config.target_count"
                                type="number"
                                min="0"
                                placeholder="0 = 不限"
                                class="flex-1 w-0 h-full bg-transparent border-0 outline-none text-xs text-text text-right appearance-none [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
                                :disabled="is_running"
                            />
                        </div>
                        <div>
                            <button
                                ref="thread_btn"
                                :disabled="is_running"
                                class="flex items-center justify-center gap-1.5 h-9 px-5 rounded-lg text-xs whitespace-nowrap text-text bg-surface-alt border border-border-dim cursor-pointer transition-colors hover:border-accent/30 focus:border-accent/40 disabled:opacity-40 disabled:cursor-not-allowed"
                                @click="thread_open = !thread_open"
                            >
                                {{ thread_count }} 线程
                                <ChevronDown :size="14" class="text-text-muted transition-transform" :class="thread_open ? 'rotate-180' : ''" />
                            </button>
                            <Teleport to="body">
                                <div v-if="thread_open" class="fixed inset-0 z-40" @click="thread_open = false" />
                                <Transition name="stream">
                                    <div
                                        v-if="thread_open && !is_running"
                                        :style="thread_popup_style"
                                        class="max-h-48 overflow-y-auto rounded-lg border border-border bg-base shadow-lg z-50"
                                    >
                                        <button
                                            v-for="n in cpu_count"
                                            :key="n"
                                            class="flex items-center justify-between w-full px-3 py-1.5 text-xs whitespace-nowrap border-0 cursor-pointer transition-colors"
                                            :class="
                                                n === thread_count
                                                    ? 'bg-accent-dim text-accent'
                                                    : 'bg-transparent text-text-muted hover:bg-surface-alt hover:text-text'
                                            "
                                            @click="
                                                thread_count = n;
                                                thread_open = false;
                                            "
                                        >
                                            {{ n }} 线程
                                            <span v-if="n === cpu_count" class="text-[10px] opacity-50">满载</span>
                                            <span v-else-if="n === Math.floor(cpu_count / 2)" class="text-[10px] opacity-50">推荐</span>
                                        </button>
                                    </div>
                                </Transition>
                            </Teleport>
                        </div>
                    </div>
                </div>

                <!-- 操作按钮 -->
                <div class="grid grid-cols-[1fr_auto] gap-2 shrink-0 mt-6">
                    <button
                        class="flex items-center justify-center gap-2 h-10 rounded-lg font-semibold text-sm cursor-pointer transition-all duration-200 border-0"
                        :class="
                            is_running
                                ? 'bg-accent/20 text-accent/60 cursor-not-allowed'
                                : 'bg-accent text-text-bright hover:shadow-[0_0_20px_oklch(0.68_0.14_250/0.3)]'
                        "
                        :disabled="is_running"
                        @click="task.run"
                    >
                        <Play v-if="!is_running" :size="16" />
                        <span v-else class="w-4 h-4 border-2 border-accent/40 border-t-accent rounded-full animate-spin" />
                        {{ is_running ? '生成中...' : '开始生成' }}
                    </button>
                    <button
                        class="flex items-center justify-center gap-1.5 h-10 px-4 rounded-lg text-sm whitespace-nowrap border cursor-pointer transition-all duration-200"
                        :class="
                            is_running
                                ? 'border-error/40 text-error bg-error/10 hover:bg-error/20'
                                : 'border-border-dim text-text-muted bg-surface-alt cursor-not-allowed opacity-40'
                        "
                        :disabled="!is_running"
                        @click="task.stop"
                    >
                        <Square :size="14" />
                        停止
                    </button>
                </div>
                <p v-if="error_text" class="m-0 text-xs text-error">{{ error_text }}</p>
            </aside>

            <!-- 右侧主区域 -->
            <div class="flex flex-col gap-4 min-h-0">
                <!-- 状态栏 -->
                <div class="grid grid-cols-4 gap-2 shrink-0">
                    <div class="px-3 py-2.5 rounded-xl border border-border bg-surface backdrop-blur-xl">
                        <span class="text-[10px] text-text-muted uppercase tracking-wider">状态</span>
                        <div class="flex items-center gap-1.5 mt-1">
                            <span
                                class="w-2 h-2 rounded-full"
                                :class="
                                    is_running
                                        ? 'bg-accent shadow-[0_0_8px_oklch(0.68_0.14_250/0.5)] animate-pulse'
                                        : progress.status === 'idle'
                                          ? 'bg-text-muted'
                                          : 'bg-hit'
                                "
                            />
                            <strong class="text-[15px] text-text-bright">{{ rule.status.value }}</strong>
                        </div>
                    </div>
                    <div class="px-3 py-2.5 rounded-xl border border-border bg-surface backdrop-blur-xl">
                        <span class="text-[10px] text-text-muted uppercase tracking-wider">速度</span>
                        <strong class="block mt-1 text-[15px] text-text-bright font-mono">{{ fmt.number(progress.speed) }}/s</strong>
                    </div>
                    <div class="px-3 py-2.5 rounded-xl border border-border bg-surface backdrop-blur-xl">
                        <span class="text-[10px] text-text-muted uppercase tracking-wider">尝试</span>
                        <strong class="block mt-1 text-[15px] text-text-bright font-mono">{{ fmt.number(progress.attempts) }}</strong>
                    </div>
                    <div class="px-3 py-2.5 rounded-xl border border-border bg-surface backdrop-blur-xl">
                        <span class="text-[10px] text-text-muted uppercase tracking-wider">命中</span>
                        <strong
                            class="block mt-1 text-[15px] font-mono"
                            :class="progress.found_count > 0 ? 'text-accent drop-shadow-[0_0_6px_oklch(0.68_0.14_250/0.4)]' : 'text-text-bright'"
                            >{{ progress.found_count }}</strong
                        >
                    </div>
                </div>

                <!-- 地址生成动画区 -->
                <div class="relative flex-1 min-h-0 rounded-xl border border-border bg-surface backdrop-blur-xl overflow-hidden">
                    <!-- 顶部扫描线 -->
                    <div
                        v-if="is_running"
                        class="absolute inset-x-0 top-0 h-0.5 pointer-events-none z-10"
                        style="
                            background: linear-gradient(90deg, transparent, oklch(0.68 0.14 250 / 0.6), transparent);
                            animation: scan_line 3s ease-in-out infinite;
                        "
                    />

                    <div class="relative z-20 flex flex-col h-full">
                        <!-- 命中卡片区 -->
                        <div v-if="matched_list.length" class="p-3 border-b border-border-dim shrink-0">
                            <p class="m-0 mb-2 text-[10px] font-semibold text-accent uppercase tracking-wider">命中地址</p>
                            <TransitionGroup name="stream" tag="div" class="flex flex-col gap-1.5">
                                <div
                                    v-for="item in matched_list.slice(0, 5)"
                                    :key="item.id"
                                    class="flex items-center h-9 px-3 rounded-lg border border-accent/20 bg-accent-dim"
                                    style="animation: match_pop 400ms cubic-bezier(0.16, 1, 0.3, 1) forwards"
                                >
                                    <code class="font-mono text-[13px]">
                                        <template v-for="(chunk, ci) in stream.highlight(item.address, item.matched_parts)" :key="ci">
                                            <span :class="chunk.hl ? 'text-hit font-bold' : 'text-text'">{{ chunk.text }}</span>
                                        </template>
                                    </code>
                                </div>
                            </TransitionGroup>
                        </div>

                        <!-- 地址流 -->
                        <div class="flex-1 min-h-0 p-3 overflow-hidden">
                            <p class="m-0 mb-2 text-[10px] font-semibold text-text-muted uppercase tracking-wider">实时地址流</p>
                            <div class="relative flex flex-col gap-px overflow-y-auto max-h-full">
                                <TransitionGroup name="stream" tag="div" class="flex flex-col gap-px">
                                    <div
                                        v-for="(item, idx) in stream_list"
                                        :key="item.id"
                                        class="flex items-center h-5.5 px-2 rounded-sm text-[11.5px] font-mono transition-opacity duration-500"
                                        :style="{ opacity: Math.max(0.08, 1 - idx * 0.06) }"
                                        :class="item.matched ? 'text-accent bg-accent-dim' : 'text-text-muted'"
                                    >
                                        <template v-for="(chunk, ci) in stream.highlight(item.address, item.matched_parts)" :key="ci">
                                            <span :class="chunk.hl ? 'text-hit font-semibold' : ''">{{ chunk.text }}</span>
                                        </template>
                                    </div>
                                </TransitionGroup>
                            </div>
                            <div v-if="!stream_list.length && !matched_list.length" class="flex items-center justify-center h-full text-sm text-text-muted/40">
                                点击「开始生成」启动地址生成
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </section>

        <!-- 历史记录弹窗 -->
        <Teleport to="body">
            <Transition name="stream">
                <div
                    v-if="history_visible"
                    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
                    @click.self="history.close"
                >
                    <div class="w-[min(1100px,94vw)] max-h-[85vh] flex flex-col rounded-xl border border-border bg-base overflow-hidden">
                        <!-- 弹窗头 -->
                        <div class="flex justify-between items-center px-5 py-3 border-b border-border-dim shrink-0">
                            <h2 class="m-0 text-[16px] font-semibold text-text-bright">历史记录</h2>
                            <div class="flex items-center gap-1">
                                <button
                                    class="inline-flex items-center gap-1.5 h-7 px-2.5 rounded-md text-[11px] leading-none font-medium bg-transparent border border-error/30 text-error/70 cursor-pointer transition-colors hover:bg-error/10 hover:text-error"
                                    @click="history.clear_all"
                                >
                                    <Trash2 :size="12" />
                                    清空全部
                                </button>
                                <button
                                    class="w-7 h-7 ml-1.5 flex items-center justify-center rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:text-text-bright hover:bg-surface-alt transition-colors"
                                    @click="history.close"
                                >
                                    <X :size="16" />
                                </button>
                            </div>
                        </div>
                        <!-- 搜索栏 -->
                        <div class="flex items-center gap-2 px-5 py-3 border-b border-border-dim shrink-0">
                            <div class="flex-1 flex items-center gap-2 h-8 px-3 rounded-lg border border-border-dim bg-surface-alt">
                                <Search :size="14" class="text-text-muted shrink-0" />
                                <input
                                    v-model="search_text"
                                    class="flex-1 bg-transparent border-0 outline-none text-sm text-text placeholder:text-text-muted/40 font-sans"
                                    placeholder="搜索地址、私钥或规则"
                                    @keydown.enter.prevent="history.load"
                                />
                            </div>
                            <button
                                class="h-8 px-3 rounded-lg bg-accent text-text-bright text-sm font-medium border-0 cursor-pointer transition-all hover:shadow-[0_0_12px_oklch(0.68_0.14_250/0.2)]"
                                @click="history.load"
                            >
                                查询
                            </button>
                        </div>
                        <!-- 表格 -->
                        <div class="flex-1 overflow-auto">
                            <table class="w-full border-collapse text-sm">
                                <thead>
                                    <tr class="text-left text-[11px] text-text-muted uppercase tracking-wider">
                                        <th class="px-4 py-2 font-semibold">地址</th>
                                        <th class="px-4 py-2 font-semibold">规则</th>
                                        <th class="px-4 py-2 font-semibold">尝试</th>
                                        <th class="px-4 py-2 font-semibold">私钥</th>
                                        <th class="px-4 py-2 font-semibold">时间</th>
                                        <th class="px-4 py-2 font-semibold w-20"></th>
                                    </tr>
                                </thead>
                                <tbody>
                                    <tr v-if="is_loading_history">
                                        <td colspan="6" class="px-4 py-8 text-center text-text-muted">加载中...</td>
                                    </tr>
                                    <tr v-else-if="!history_list.length">
                                        <td colspan="6" class="px-4 py-8 text-center text-text-muted">暂无记录</td>
                                    </tr>
                                    <tr v-for="row in history_list" :key="row.id" class="border-t border-border-dim hover:bg-surface-alt/50 transition-colors">
                                        <td class="px-4 py-2 font-mono text-xs">
                                            <template v-for="(chunk, ci) in stream.highlight(row.address, row.matched_parts)" :key="ci">
                                                <span :class="chunk.hl ? 'text-hit font-bold' : ''">{{ chunk.text }}</span>
                                            </template>
                                        </td>
                                        <td class="px-4 py-2">
                                            <div class="flex flex-wrap gap-1">
                                                <span
                                                    v-for="r in row.matched_rules"
                                                    :key="r"
                                                    class="inline-flex items-center h-5 px-1.5 rounded text-[10px] leading-none bg-surface-alt text-text-muted"
                                                    >{{ r }}</span
                                                >
                                            </div>
                                        </td>
                                        <td class="px-4 py-2 font-mono text-xs text-text-muted">{{ fmt.number(row.attempts) }}</td>
                                        <td class="px-4 py-2 font-mono text-xs text-text-muted">{{ fmt.short_key(row.private_key) }}</td>
                                        <td class="px-4 py-2 text-xs text-text-muted">{{ fmt.date(row.created_at) }}</td>
                                        <td class="px-4 py-2">
                                            <div class="flex items-center gap-0.5">
                                                <button
                                                    class="w-7 h-7 flex items-center justify-center rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:text-accent hover:bg-accent-dim transition-colors"
                                                    title="复制地址 + 密钥"
                                                    @click="history.copy(row)"
                                                >
                                                    <Copy :size="13" />
                                                </button>
                                                <button
                                                    class="w-7 h-7 flex items-center justify-center rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:text-accent hover:bg-accent-dim transition-colors"
                                                    title="二维码"
                                                    @click="show_qr(row)"
                                                >
                                                    <QrCode :size="13" />
                                                </button>
                                                <button
                                                    class="w-7 h-7 flex items-center justify-center rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:text-error hover:bg-error/10 transition-colors"
                                                    title="删除"
                                                    @click="history.remove(row.id)"
                                                >
                                                    <Trash2 :size="13" />
                                                </button>
                                            </div>
                                        </td>
                                    </tr>
                                </tbody>
                            </table>
                        </div>
                    </div>
                </div>
            </Transition>
        </Teleport>

        <!-- 二维码弹窗 -->
        <Teleport to="body">
            <Transition name="stream">
                <div v-if="qr_wallet" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm" @click.self="qr_wallet = null">
                    <div class="w-80 rounded-2xl border border-border bg-surface p-5 shadow-2xl">
                        <div class="flex items-center justify-between mb-4">
                            <h3 class="m-0 text-sm font-semibold text-text">钱包二维码</h3>
                            <button
                                class="w-7 h-7 flex items-center justify-center rounded-lg bg-transparent border-0 text-text-muted cursor-pointer hover:text-text hover:bg-surface-alt transition-colors"
                                @click="qr_wallet = null"
                            >
                                <X :size="14" />
                            </button>
                        </div>
                        <div class="flex gap-1 p-1 rounded-lg bg-surface-alt mb-4">
                            <button
                                class="flex-1 h-8 rounded-md text-xs font-medium border-0 cursor-pointer transition-all"
                                :class="qr_mode === 'private_key' ? 'bg-accent text-text-bright shadow-sm' : 'bg-transparent text-text-muted hover:text-text'"
                                @click="qr_mode = 'private_key'"
                            >
                                私钥（导入钱包）
                            </button>
                            <button
                                class="flex-1 h-8 rounded-md text-xs font-medium border-0 cursor-pointer transition-all"
                                :class="qr_mode === 'address' ? 'bg-accent text-text-bright shadow-sm' : 'bg-transparent text-text-muted hover:text-text'"
                                @click="qr_mode = 'address'"
                            >
                                地址（收款/观察）
                            </button>
                        </div>
                        <div class="flex justify-center p-4 rounded-xl bg-surface-alt">
                            <canvas ref="qr_canvas" />
                        </div>
                        <p class="mt-3 mb-0 text-center text-[11px] text-text-muted font-mono break-all leading-relaxed">
                            {{ qr_mode === 'address' ? qr_wallet.address : fmt.short_key(qr_wallet.private_key) }}
                        </p>
                        <p class="mt-2 mb-0 text-center text-[10px] text-text-muted/60">
                            {{ qr_mode === 'private_key' ? '使用 imToken / TokenPocket 扫码可直接导入' : '扫码添加为观察钱包或用于收款' }}
                        </p>
                    </div>
                </div>
            </Transition>
        </Teleport>
    </main>

    <!-- 消息提示 -->
    <Teleport to="body">
        <TransitionGroup name="toast" tag="div" class="fixed top-0 left-1/2 -translate-x-1/2 z-9999 flex flex-col items-center gap-2 pt-4 pointer-events-none">
            <div
                v-for="item in toast.list.value"
                :key="item.id"
                class="pointer-events-auto px-4 py-2.5 rounded-xl text-xs font-medium shadow-lg backdrop-blur-xl border"
                :class="{
                    'bg-emerald-500/15 text-emerald-400 border-emerald-500/30': item.type === 'success',
                    'bg-red-500/15 text-red-400 border-red-500/30': item.type === 'error',
                    'bg-blue-500/15 text-blue-400 border-blue-500/30': item.type === 'info',
                }"
            >
                {{ item.text }}
            </div>
        </TransitionGroup>
    </Teleport>
</template>
