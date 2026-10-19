export type Locale = 'de' | 'en' | 'fr' | 'es';
export type Parameters = Record<string, string | number>;
export type TranslationCatalogue = Record<Exclude<Locale, 'de'>, Record<string, string>>;
