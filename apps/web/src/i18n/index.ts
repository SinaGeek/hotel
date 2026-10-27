// Lightweight i18next setup: Turkish + English (README §4)
import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'

const tr = {
  app: { title: 'HMS Otel Yönetimi' },
  login: {
    hotelId: 'Tesis Kodu',
    username: 'Kullanıcı Adı',
    password: 'Şifre',
    signIn: 'Giriş Yap',
    mustChangePassword: 'İlk girişte şifrenizi değiştirmeniz gerekiyor',
  },
  nav: { dashboard: 'Ana Sayfa' },
  common: { loading: 'Yükleniyor...', error: 'Hata', save: 'Kaydet', cancel: 'İptal' },
}

const en = {
  app: { title: 'HMS Hotel Management' },
  login: {
    hotelId: 'Facility Code',
    username: 'Username',
    password: 'Password',
    signIn: 'Sign in',
    mustChangePassword: 'You must change your password on first login',
  },
  nav: { dashboard: 'Dashboard' },
  common: { loading: 'Loading...', error: 'Error', save: 'Save', cancel: 'Cancel' },
}

i18n.use(initReactI18next).init({
  resources: { tr: { translation: tr }, en: { translation: en } },
  lng: localStorage.getItem('lang') || 'tr',
  fallbackLng: 'en',
  interpolation: { escapeValue: false },
})

export default i18n
