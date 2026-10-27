import { useTranslation } from 'react-i18next'

export function Dashboard() {
  const { t } = useTranslation()
  return <h2>{t('nav.dashboard')}</h2>
}
