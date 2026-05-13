from django.contrib import admin
from django.contrib.auth.admin import UserAdmin
from django.contrib.auth import get_user_model
from django.contrib.auth.models import Group
from django.http import HttpResponseRedirect
from django.urls import reverse
from django.utils.html import format_html
from django.utils.translation import gettext_lazy as _

from .models import Role, ActivityLog, PermissionDefinition

User = get_user_model()

@admin.register(PermissionDefinition)
class PermissionDefinitionAdmin(admin.ModelAdmin):
    list_display = ('name', 'codename')
    search_fields = ('name', 'codename')

@admin.register(User)
class CustomUserAdmin(UserAdmin):
    list_display = ('username', 'email', 'first_name', 'last_name', 'role', 'is_active', 'is_staff', 'password_change_link')
    list_filter = ('is_active', 'is_staff', 'is_superuser', 'role')
    search_fields = ('username', 'first_name', 'last_name', 'email')
    
    fieldsets = (
        (None, {'fields': ('username', 'password')}),
        (_('Personal Info'), {'fields': ('first_name', 'last_name', 'email')}),
        (_('Role and Access'), {'fields': ('is_active', 'is_staff', 'is_superuser', 'role', 'hotel_id')}),
        (_('Important dates'), {'fields': ('last_login', 'date_joined')}),
    )

    def password_change_link(self, obj):
        url = reverse('admin:auth_user_password_change', args=[obj.id])
        return format_html('<a href="{}" style="color: #3b82f6;">Change Password 🔑</a>', url)
    password_change_link.short_description = _('Password')
    
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
            self.message_user(request, _('User %s approved and activated as staff.') % user.username)
        elif action == 'deactivate':
            user.is_staff = False
            user.is_active = False
            user.role = None
            user.save()
            self.message_user(request, _('User %s left the job.') % user.username)
        elif action == 'set_password':
            return HttpResponseRedirect(reverse('admin:auth_user_password_change', args=[user_id]))
        elif action.startswith('set_role_'):
            role_id = action.split('_')[2]
            try:
                role = Role.objects.get(id=role_id)
                user.role = role
                user.save()
                self.message_user(request, _('User %s assigned to role %s.') % (user.username, role.name))
            except Role.DoesNotExist:
                self.message_user(request, _('Role not found.'), level='error')
        return HttpResponseRedirect(reverse('admin:accounts_user_changelist'))

@admin.register(Role)
class RoleAdmin(admin.ModelAdmin):
    list_display = ('name', 'description')
    search_fields = ('name',)
    filter_horizontal = ('permissions_list',)
    
    fieldsets = (
        (None, {'fields': ('name', 'description')}),
        (_('Access Rights'), {
            'fields': ('permissions_list',),
            'description': _('Select permissions from the list below.')
        }),
    )

@admin.register(ActivityLog)
class ActivityLogAdmin(admin.ModelAdmin):
    list_display = ('user', 'action', 'timestamp', 'is_visible_to_user')
    list_filter = ('is_visible_to_user', 'timestamp')
    search_fields = ('user__email', 'action')
