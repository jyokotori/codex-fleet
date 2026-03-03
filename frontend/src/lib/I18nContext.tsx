import { createContext, useState, ReactNode } from 'react'
import {
  Locale,
  Translations,
  translations,
  getStoredLocale,
  setStoredLocale,
} from './i18n'

interface I18nContextValue {
  locale: Locale
  t: Translations
  setLocale: (locale: Locale) => void
}

export const I18nContext = createContext<I18nContextValue>({
  locale: 'en',
  t: translations.en,
  setLocale: () => {},
})

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(getStoredLocale)

  function setLocale(next: Locale) {
    setLocaleState(next)
    setStoredLocale(next)
  }

  return (
    <I18nContext.Provider value={{ locale, t: translations[locale] as unknown as Translations, setLocale }}>
      {children}
    </I18nContext.Provider>
  )
}
