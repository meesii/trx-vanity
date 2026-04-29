import { ref } from 'vue';

/**
 * @type {Object} Toast 消息项
 */
interface ToastItem {
    id: number;
    text: string;
    type: 'success' | 'error' | 'info';
}

let seq = 0;
const list = ref<ToastItem[]>([]);

/**
 * @type {Object} 全局消息提示
 */
export const toast = {
    list,
    show(text: string, type: ToastItem['type'] = 'success', duration = 2500) {
        const id = ++seq;
        list.value.push({ id, text, type });
        setTimeout(() => {
            list.value = list.value.filter((t) => t.id !== id);
        }, duration);
    },
    success(text: string) {
        toast.show(text, 'success');
    },
    error(text: string) {
        toast.show(text, 'error', 4000);
    },
    info(text: string) {
        toast.show(text, 'info');
    },
};
