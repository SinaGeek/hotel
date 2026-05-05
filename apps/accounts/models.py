from django.contrib.auth.models import AbstractUser
from django.db import models
from apps.core.models import TimeStampedModel, SoftDeleteModel

class Role(TimeStampedModel):
    name=models.CharField(max_length=50, unique=True, verbose_name="Role name")
    description = models.TextField(blank=True, verbose_name="Duty description")
    permissions=models.JSONField(default=list, verbose_name="Acess", help_text="List of Acceses as JSON. Ex.: ['can_view_rooms', 'can_edit_inventory']")

    class Meta:
        verbose_name = "Role"
        verbose_name_plural = "Roles"

class User(AbstractUser, SoftDeleteModel):
    hotel_id=models.BigIntegerField(db_index=True, null=True, blank=True, verbose_name="Hotel ID")
    role=models.ForeignKey(Role, null=True, blank=True, on_delete=models.SET_NULL, verbose_name="Role")

    class Meta:
        verbose_name = "User"
        verbose_name_plural = "Users"

class ActivityLog(models.Model):
    user = models.ForeignKey(User, on_delete=models.CASCADE, related_name='activity_logs', verbose_name="User")
    action = models.TextField(verbose_name="Operation")
    timestamp = models.DateTimeField(auto_now_add=True, verbose_name="Timr stamp")
    is_visible_to_user = models.BooleanField(default=False, verbose_name="Visible to user")

    class Meta:
        verbose_name = "Activity Log"
        verbose_name_plural = "Activity Logs"
        ordering = ['-timestamp']