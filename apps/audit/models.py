from django.db import models
class AuditLog(models.Model):
    user=models.ForeignKey('accounts.User',null=True,on_delete=models.SET_NULL)
    action=models.CharField(max_length=50); entity_type=models.CharField(max_length=80); entity_id=models.CharField(max_length=50)
    before=models.JSONField(default=dict); after=models.JSONField(default=dict); timestamp=models.DateTimeField(auto_now_add=True)
