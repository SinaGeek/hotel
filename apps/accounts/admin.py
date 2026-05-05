from django.contrib import admin
from django.contrib.auth.admin import UserAdmin
from django.contrib.auth import get_user_model
from django.contrib.auth.models import Group
from django.http import HttpResponseRedirect
from django.urls import reverse
from django.utils.html import format_html

from .models import Role, ActivityLog

User = get_user_model()

@admin.register(User)
class CustomUserAdmin(UserAdmin):
    list_display = ('username', 'email', 'first_name', 'last_name', 'role', 'is_active', 'is_staff', 'password_change_link')
    # list_editable = ('role',) # Removed to use actions dropdown
    list_display_links = None  # Remove edit links
    list_filter = ('is_active', 'is_staff', 'is_superuser', 'role')
    search_fields = ('username', 'first_name', 'last_name', 'email')
    
    fieldsets = (
        (None, {'fields': ('username', 'password')}),
        ('اطلاعات فردی', {'fields': ('first_name', 'last_name', 'email')}),
        ('نقش و دسترسی', {'fields': ('is_active', 'is_staff', 'is_superuser', 'role', 'hotel_id')}),
        ('تاریخچه‌ها', {'fields': ('last_login', 'date_joined')}),
    )

    def password_change_link(self, obj):
        url = reverse('admin:auth_user_password_change', args=[obj.id])
        return format_html('<a href="{}" style="color: #3b82f6;">Change Password 🔑</a>', url)
    password_change_link.short_description = 'Password'
    

    def get_urls(self):
        from django.urls import path
        urls = super().get_urls()
        custom_urls = [
            path('execute_action/<int:user_id>/<str:action>/', self.execute_action_view, name='execute_user_action'),
        ]
        return custom_urls + urls

    def execute_action_view(self, request, user_id, action):
        user = User.objects.get(id=user_id)
        if action == 'approve':
            user.is_active = True
            user.is_staff = True
            user.save()
            self.message_user(request, f'User {user.username} approved and activated as staff.')
        elif action == 'deactivate':
            user.is_staff = False
            user.is_active = False
            user.role = None
            user.save()
            self.message_user(request, f'User {user.username} leaved the Job.')
        elif action == 'set_password':
            return HttpResponseRedirect(reverse('admin:auth_user_password_change', args=[user_id]))
        elif action.startswith('set_role_'):
            role_id = action.split('_')[2]
            try:
                role = Role.objects.get(id=role_id)
                user.role = role
                user.save()
                self.message_user(request, f'User {user.username} assign to role {role.name}.')
            except Role.DoesNotExist:
                self.message_user(request, 'Role not found.', level='error')
        return HttpResponseRedirect(reverse('admin:accounts_user_changelist'))

@admin.register(Role)
class RoleAdmin(admin.ModelAdmin):
    list_display = ('name', 'description')
    search_fields = ('name',)

@admin.register(ActivityLog)
class ActivityLogAdmin(admin.ModelAdmin):
    list_display = ('user', 'action', 'timestamp', 'is_visible_to_user')
    list_filter = ('is_visible_to_user', 'timestamp')
    search_fields = ('user__email', 'action')


