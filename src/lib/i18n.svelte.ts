export type Lang = 'en' | 'es'

export const i18n = $state({ lang: 'en' as Lang })

/** Inline translation: `t('Back up now', 'Respaldar ahora')`. Reactive, since it reads `i18n.lang`. */
export const t = (en: string, es: string) => (i18n.lang === 'es' ? es : en)

/** Fixed decimals with the language's separator: 1.5 / 1,5 */
export const dec = (n: number, d = 1) => (i18n.lang === 'es' ? n.toFixed(d).replace('.', ',') : n.toFixed(d))
