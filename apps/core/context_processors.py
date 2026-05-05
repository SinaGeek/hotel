
from .translation_data import TRANSLATIONS

def translation_processor(request):
    # Determine current language (default to 'fa')
    lang_code = request.LANGUAGE_CODE if hasattr(request, 'LANGUAGE_CODE') else 'fa'
    if lang_code not in TRANSLATIONS['_meta']:
        lang_code = 'fa'
    
    # Extract strings for current language
    lang_strings = {}
    for key, values in TRANSLATIONS.items():
        if key == '_meta':
            continue
        lang_strings[key] = values.get(lang_code, values.get('en', ''))
    
    return {
        't': lang_strings,
        'lang_meta': TRANSLATIONS['_meta'][lang_code],
        'current_lang': lang_code,
        'available_langs': TRANSLATIONS['_meta']
    }
