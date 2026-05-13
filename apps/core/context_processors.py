
from .translation_data import TRANSLATIONS

def translation_processor(request):
    from django.utils.translation import get_language
    lang_code = get_language() or 'fa'
    # Use only the base language code (e.g., 'en' from 'en-us')
    lang_code = lang_code.split('-')[0]
    
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
        'available_langs': TRANSLATIONS['_meta'],
        'view_type': request.GET.get('view', 'large'),
    }
