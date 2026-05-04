from django.db import models
from apps.core.models import HotelScopedModel, SoftDeleteModel
class Guest(HotelScopedModel, SoftDeleteModel):
    first_name=models.CharField(max_length=60); last_name=models.CharField(max_length=60); email=models.EmailField(blank=True)
class GuestDocument(HotelScopedModel):
    guest=models.ForeignKey(Guest,on_delete=models.CASCADE,related_name='documents')
    doc_type=models.CharField(max_length=40); doc_number=models.CharField(max_length=80); metadata=models.JSONField(default=dict)
