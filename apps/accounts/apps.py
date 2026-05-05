from django.apps import AppConfig
class AccountsConfig(AppConfig):
    default_auto_field='django.db.models.BigAutoField'
    name='apps.accounts'

    def ready(self):
        # We use a post_migrate signal or just check here (check is simpler for this context)
        # However, to avoid circular imports and db issues during migrations, we use a internal function
        import sys
        if 'runserver' in sys.argv:
            try:
                from .models import Role
                default_roles = [
                    ('مدیر سیستم', 'دسترسی کامل به تمام بخش‌های هتل'),
                    ('پذیرش', 'مدیریت رزروها، اتاق‌ها و مسافران'),
                    ('گارسون', 'ثبت سفارشات رستوران و POS'),
                    ('خانه داری', 'مدیریت وضعیت نظافت اتاق‌ها'),
                    ('فروشنده', 'مدیریت انبار و فروش کالاها'),
                ]
                for name, desc in default_roles:
                    Role.objects.get_or_create(name=name, defaults={'description': desc})
            except:
                pass
