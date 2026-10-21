import { locale } from '@tauri-apps/plugin-os';

import { en } from './en';
import { ptBR } from './pt-BR';

const translations: Record<string, typeof en> = {
  en,
  'pt-BR': ptBR,
}

async function getStrings() {
  const systemLang = await locale().catch(() => null);
  const translation = translations[systemLang || 'en'] ?? translations.en;
  return translation;
}

export const strings = await getStrings();
